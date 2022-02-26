use std::process::Command;

use actix_web::{
    get,
    http::header::{
        Accept, CacheControl, CacheDirective, ContentType, EntityTag, Header, IfNoneMatch,
        CACHE_CONTROL, CONTENT_TYPE, ETAG, LOCATION,
    },
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use badge::{Badge, BadgeOptions};
use cached::Cached;
use once_cell::sync::Lazy;
use tempfile::TempDir;
use tokei::{Language, Languages};

const BILLION: usize = 1_000_000_000;
const BLANKS: &str = "blank lines";
const BLUE: &str = "#007ec6";
const CODE: &str = "lines of code";
const COMMENTS: &str = "comments";
const FILES: &str = "files";
const HASH_LENGTH: usize = 40;
const LINES: &str = "total lines";
const MILLION: usize = 1_000_000;
const THOUSAND: usize = 1_000;
const DAY_IN_SECONDS: u64 = 24 * 60 * 60;

static CONTENT_TYPE_SVG: Lazy<ContentType> =
    Lazy::new(|| ContentType("image/svg+xml".parse().unwrap()));

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    HttpServer::new(|| {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .service(redirect_index)
            .service(create_badge)
    })
    .bind(("0.0.0.0", 8000))?
    .run()
    .await
}

#[get("/")]
async fn redirect_index() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .insert_header((LOCATION, "https://github.com/XAMPPRocky/tokei"))
        .finish()
}

macro_rules! respond {
    ($status:ident) => {{
        HttpResponse::$status().finish()
    }};

    ($status:ident, $body:expr) => {{
        HttpResponse::$status()
            .set(CONTENT_TYPE_SVG.clone())
            .body($body)
    }};

    ($status:ident, $accept:expr, $body:expr, $etag:expr) => {{
        HttpResponse::$status()
            .insert_header((CACHE_CONTROL, CacheControl(vec![CacheDirective::NoCache])))
            .insert_header((ETAG, EntityTag::new(false, $etag)))
            .insert_header((
                CONTENT_TYPE,
                if $accept == ContentType::json() {
                    ContentType::json()
                } else {
                    CONTENT_TYPE_SVG.clone()
                },
            ))
            .body($body)
    }};
}

#[derive(serde::Deserialize)]
struct BadgeQuery {
    category: Option<String>,
}

#[get("/b1/{domain}/{user}/{repo}")]
async fn create_badge(
    request: HttpRequest,
    path: web::Path<(String, String, String)>,
    web::Query(query): web::Query<BadgeQuery>,
) -> actix_web::Result<HttpResponse> {
    let (domain, user, repo) = path.into_inner();
    let category = query.category.unwrap_or_else(|| String::from("lines"));

    let content_type = if let Ok(accept) = Accept::parse(&request) {
        if accept == Accept::json() {
            ContentType::json()
        } else {
            CONTENT_TYPE_SVG.clone()
        }
    } else {
        CONTENT_TYPE_SVG.clone()
    };

    let mut domain = percent_encoding::percent_decode_str(&domain).decode_utf8()?;

    // For backwards compatibility if a domain isn't specified we append `.com`.
    if !domain.contains('.') {
        domain += ".com";
    }

    let url = format!("https://{}/{}/{}", domain, user, repo);
    let ls_remote = Command::new("git").arg("ls-remote").arg(&url).output()?;
    let sha: String = ls_remote
        .stdout
        .iter()
        .position(|&b| b == b'\t')
        .filter(|i| *i == HASH_LENGTH)
        .map(|i| (&ls_remote.stdout[..i]).to_owned())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;

    if let Ok(if_none_match) = IfNoneMatch::parse(&request) {
        let sha_tag = EntityTag::new(false, sha.clone());
        let found_match = match if_none_match {
            IfNoneMatch::Any => false,
            IfNoneMatch::Items(items) => items.iter().any(|etag| etag.weak_eq(&sha_tag)),
        };

        if found_match {
            CACHE
                .lock()
                .unwrap()
                .cache_get(&repo_identifier(&url, &sha));
            log::info!("{}#{} Not Modified", url, sha);
            return Ok(respond!(NotModified));
        }
    }

    let entry = get_statistics(&url, &sha).map_err(actix_web::error::ErrorBadRequest)?;

    if entry.was_cached {
        log::info!("{}#{} Cache hit", url, sha);
    }

    let stats = entry.value;

    log::info!(
        "{url}#{sha} - Lines {lines} Code {code} Comments {comments} Blanks {blanks}",
        url = url,
        sha = sha,
        lines = stats.lines,
        code = stats.code,
        comments = stats.comments,
        blanks = stats.blanks
    );

    let badge = make_badge(&content_type, &stats, &category)?;

    Ok(respond!(Ok, content_type, badge, sha))
}

fn repo_identifier(url: &str, sha: &str) -> String {
    format!("{}#{}", url, sha)
}

#[cached::proc_macro::cached(
    name = "CACHE",
    result = true,
    with_cached_flag = true,
    type = "cached::TimedSizedCache<String, cached::Return<Language>>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(1000, DAY_IN_SECONDS) }",
    convert = r#"{ repo_identifier(url, _sha) }"#
)]
fn get_statistics(url: &str, _sha: &str) -> eyre::Result<cached::Return<Language>> {
    log::info!("{} - Cloning", url);
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args(&["clone", url, temp_path, "--depth", "1"])
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

fn make_badge(
    content_type: &ContentType,
    stats: &Language,
    category: &str,
) -> actix_web::Result<String> {
    if *content_type == ContentType::json() {
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
