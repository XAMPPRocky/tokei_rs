#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use std::{io::Cursor, process::Command};

use badge::{Badge, BadgeOptions};
use once_cell::sync::Lazy;
use rocket::{
    http::{hyper::header::EntityTag, Accept, ContentType, Status},
    response::Redirect,
    Response,
};
use tempfile::TempDir;
use tokei::{Language, Languages};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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

static BAD_URL_BADGE: Lazy<String> = Lazy::new(|| {
    let options = BadgeOptions {
        subject: String::from("Error"),
        status: String::from("Incorrect URL"),
        color: String::from(RED),
    };

    Badge::new(options).unwrap().to_svg()
});

static INVALID_SHA_BADGE: Lazy<String> = Lazy::new(|| {
    let options = BadgeOptions {
        subject: String::from("Error"),
        status: String::from("Invalid commit SHA"),
        color: String::from(RED),
    };

    Badge::new(options).unwrap().to_svg()
});

static PROCESSING_BADGE: Lazy<String> = Lazy::new(|| {
    let options = BadgeOptions {
        subject: String::from("Error"),
        status: String::from("Procsssing…"),
        color: String::from(RED),
    };

    Badge::new(options).unwrap().to_svg()
});

fn main() {
    dotenv::dotenv().unwrap();
    rocket::ignite().mount("/", routes![index, badge]).launch();
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


#[get("/b1/<domain>/<user>/<repo>?<category>")]
fn badge<'response>(
    accept_header: &Accept,
    if_none_match: IfNoneMatch,
    domain: String,
    user: String,
    repo: String,
    category: Option<String>,
) -> Result<Response<'response>> {
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
    let sha = match String::from_utf8(stdout[..end_of_sha].to_owned()) {
        Ok(s) => s,
        Err(_) => return respond!(Status::BadRequest, &**INVALID_SHA_BADGE),
    };

    let entry = get_statistics(&url, &sha)?;

    if entry.was_cached {
        log::info!("{}#{} Cache hit", url, sha);

        if let IfNoneMatch(Some(etag)) = if_none_match {
            let sha_tag = EntityTag::new(false, sha.clone());
            if sha_tag.weak_eq(&etag) {
                log::info!("{}#{} Not Modified", url, sha);
                return respond!(Status::NotModified);
            }
        }
    }

    let stats = entry.value;

    log::info!(
        "{url}#{sha} - Lines {lines} Code {code} Comments {comments} Blanks {blanks}",
        url=url,
        sha=sha,
        lines=stats.lines,
        code=stats.code,
        comments=stats.comments,
        blanks=stats.blanks
    );

    let badge = make_badge(accept_header, &stats, &category)?;

    respond!(
        Status::Ok,
        accept_header,
        badge,
        sha
    )
}

#[cached::proc_macro::cached(
    result = true,
    with_cached_flag = true,
    type = "cached::TimedSizedCache<String, cached::Return<Language>>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(1000, 86400) }",
    convert = r#"{ format!("{}{}", url, _sha) }"#,
)]
fn get_statistics(url: &str, _sha: &str) -> Result<cached::Return<Language>> {
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

    for (_, language) in languages {
        stats += language;
    }

    for stat in &mut stats.stats {
        stat.name = stat.name.strip_prefix(temp_path)?.to_owned();
    }

    Ok(cached::Return::new(stats))
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

fn make_badge(accept: &Accept, stats: &Language, category: &str) -> Result<String> {
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
