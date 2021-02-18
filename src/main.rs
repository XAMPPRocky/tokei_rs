#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use std::{env, io::Cursor, process::Command};

use badge::{Badge, BadgeOptions};
use lazy_static::lazy_static;
use r2d2_redis::RedisConnectionManager;
use redis::Commands;
use rocket::{
    http::{hyper::header::EntityTag, Accept, ContentType, Status},
    response::Redirect,
    Response, State,
};
use tempfile::TempDir;
use tokei::{Language, Languages};

type Result<T> = std::result::Result<T, failure::Error>;

const BILLION: usize = 1_000_000_000;
const BLANKS: &str = "blank lines";
const BLUE: &str = "#007ec6";
const GREY: &str = "#9e9e9e";
const CODE: &str = "lines of code";
const COMMENTS: &str = "comments";
const FILES: &str = "files";
const HASH_LENGTH: usize = 40;
const LINES: &str = "total lines";
const MILLION: usize = 1_000_000;
const RED: &str = "#e05d44";
const THOUSAND: usize = 1_000;
const DAY_IN_SECONDS: usize = 24 * 60 * 60;

lazy_static! {
    static ref BAD_URL_BADGE: String = {
        let options = BadgeOptions {
            subject: String::from("Error"),
            status: String::from("Incorrect URL"),
            color: String::from(RED),
        };

        Badge::new(options).unwrap().to_svg()
    };
    static ref REDIS_URL: String = env::var("REDIS_URL").unwrap();
}

macro_rules! respond {
    ($status:expr) => {{
        let mut response = Response::new();
        response.set_status($status);
        Ok(response)
    }};

    ($status:expr, $body:expr) => {{
        let mut response = Response::new();
        response.set_status($status);
        response.set_sized_body(Cursor::new($body));
        response.set_header(ContentType::SVG);
        Ok(response)
    }};

    ($status:expr, $accept:expr, $body:expr, $etag:expr) => {{
        use rocket::http::hyper::header::{CacheControl, CacheDirective, ETag};

        let mut response = Response::new();
        response.set_status($status);
        response.set_sized_body(Cursor::new($body));
        response.set_header(if *$accept == Accept::JSON {
            ContentType::JSON
        } else {
            ContentType::SVG
        });
        response.set_header(CacheControl(vec![CacheDirective::NoCache]));
        response.set_header(ETag(EntityTag::new(false, $etag)));
        Ok(response)
    }};
}

fn main() {
    dotenv::dotenv().unwrap();
    let manager = RedisConnectionManager::new(&**REDIS_URL).unwrap();
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    rocket::ignite()
        .manage(pool)
        .mount("/", routes![index, stats_badge, lang_badge])
        .launch();
}

#[get("/")]
fn index() -> Redirect {
    Redirect::permanent("https://github.com/XAMPPRocky/tokei")
}

struct IfNoneMatch(Option<EntityTag>);

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for IfNoneMatch {
    type Error = ();

    fn from_request(
        request: &'a rocket::Request<'r>,
    ) -> rocket::request::Outcome<Self, Self::Error> {
        rocket::Outcome::Success(Self(
            request
                .headers()
                .get("If-None-Match")
                .next()
                .and_then(|s| s.parse().ok()),
        ))
    }
}

#[get("/b1/<domain>/<user>/<repo>?<category>")]
fn stats_badge<'a, 'b>(
    accept_header: &Accept,
    if_none_match: IfNoneMatch,
    domain: String,
    user: String,
    repo: String,
    category: Option<String>,
    pool: State<r2d2::Pool<RedisConnectionManager>>,
) -> Result<Response<'b>> {
    let category = category.unwrap_or(String::from("lines"));

    let mut domain = percent_encoding::percent_decode_str(&domain).decode_utf8()?;

    // For backwards compatability if a domain isn't specified we append `.com`.
    if !domain.contains(".") {
        domain += ".com";
    }

    let url = format!("https://{}/{}/{}", domain, user, repo);
    let ls_remote = Command::new("git").arg("ls-remote").arg(&url).output()?;
    let stdout = ls_remote.stdout;
    let end_of_sha = match stdout.iter().position(|&b| b == b'\t') {
        Some(index) if index == HASH_LENGTH => index,
        _ => return respond!(Status::BadRequest, &**BAD_URL_BADGE),
    };
    let hash = String::from_utf8_lossy(&stdout[..end_of_sha]);

    if let IfNoneMatch(Some(etag)) = if_none_match {
        let hash = EntityTag::new(false, hash.to_owned().into_owned());
        if hash.weak_eq(&etag) {
            log::info!("Not Modified");
            return respond!(Status::NotModified);
        }
    }

    let mut redis = pool.get()?;

    if let Some(stats) = redis
        .get::<_, Option<String>>(&*hash)?
        .and_then(|s| serde_json::from_str::<Language>(&s).ok())
    {
        log::info!("Found cached entry.");
        log_total(&stats, &url);
        return respond!(
            Status::Ok,
            accept_header,
            make_stats_badge(accept_header, stats, &category)?,
            (&*hash).to_owned()
        );
    }

    log::info!("{} - Cloning", url);
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args(&["clone", &url, &temp_path, "--depth", "1"])
        .output()?;

    let mut stats = Language::new();
    let mut languages = Languages::new();
    log::info!("{} - Getting Statistics", url);
    languages.get_statistics(&[temp_path], &[], &tokei::Config::default());

    // There seems to be a race condition where multiple requests to the same
    // repo can fail and report `0` and then become cached, this solves it
    // by checking if we actually found anything first before trying to cache.
    if languages.is_empty() {
        let options = BadgeOptions {
            subject: String::from(category),
            status: String::from("Processing..."),
            color: String::from(GREY),
        };

        return respond!(
            Status::Ok,
            accept_header,
            Badge::new(options).unwrap().to_svg(),
            (&*hash).to_owned()
        );
    }

    for (_, language) in languages {
        stats += language;
    }

    for stat in &mut stats.stats {
        stat.name = stat.name.strip_prefix(temp_path)?.to_owned();
    }

    log_total(&stats, &url);
    redis.set(&*hash, serde_json::to_string(&stats)?)?;

    respond!(
        Status::Ok,
        accept_header,
        make_stats_badge(accept_header, stats, &category)?,
        (&*hash).to_owned()
    )
}

#[get("/b2/<domain>/<user>/<repo>")]
fn lang_badge<'a, 'b>(
    accept_header: &Accept,
    if_none_match: IfNoneMatch,
    domain: String,
    user: String,
    repo: String,
    pool: State<r2d2::Pool<RedisConnectionManager>>,
) -> Result<Response<'b>> {
    respond!(Status::Ok)
}

fn log_total(stats: &Language, url: &str) {
    log::info!(
        "{} - Lines {} Code {} Comments {} Blanks {}",
        url,
        stats.lines,
        stats.code,
        stats.comments,
        stats.blanks
    );
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

fn make_stats_badge(accept: &Accept, stats: Language, category: &str) -> Result<String> {
    if *accept == Accept::JSON {
        return Ok(serde_json::to_string(&stats)?);
    }

    let (amount, label) = match &*category {
        "code" => (stats.code, CODE),
        "files" => (stats.stats.len(), FILES),
        "blanks" => (stats.blanks, BLANKS),
        "comments" => (stats.comments, COMMENTS),
        _ => (stats.lines, LINES),
    };

    let amount = if amount >= BILLION {
        format!("{:.1}B", trim_and_float(amount, BILLION))
    } else if amount >= MILLION {
        format!("{:.1}M", trim_and_float(amount, MILLION))
    } else if amount >= THOUSAND {
        format!("{:.1}K", trim_and_float(amount, THOUSAND))
    } else {
        amount.to_string()
    };

    let options = BadgeOptions {
        subject: String::from(label),
        status: amount,
        color: String::from(BLUE),
    };

    Ok(Badge::new(options).unwrap().to_svg())
}
