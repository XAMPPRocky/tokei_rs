use std::process::Command;

use actix_http::http::header::{
    Accept, CacheControl, CacheDirective, ContentType, EntityTag, Header, IfNoneMatch,
    CACHE_CONTROL, CONTENT_TYPE, ETAG, LOCATION,
};
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer};
use badge::{Badge, BadgeOptions};
use cached::Cached;
use once_cell::sync::Lazy;
use tempfile::TempDir;
use tokei::{Language, LanguageType, Languages};

const BILLION: usize = 1_000_000_000;
const BLANKS: &str = "blank lines";
const BLUE: &str = "#007ec6";
const CODE: &str = "lines of code";
const COMMENTS: &str = "comments";
const FILES: &str = "files";
const PRIMARY_LANG: &str = "primary language";
const HASH_LENGTH: usize = 40;
const LINES: &str = "total lines";
const MILLION: usize = 1_000_000;
const THOUSAND: usize = 1_000;
const DAY_IN_SECONDS: u64 = 24 * 60 * 60;

static CONTENT_TYPE_SVG: Lazy<ContentType> =
    Lazy::new(|| ContentType("image/svg".parse().unwrap()));

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    dotenv::dotenv().unwrap();

    HttpServer::new(|| {
        App::new()
            .wrap(actix_web::middleware::Logger::default())
            .service(redirect_index)
            .service(create_stats_badge)
            .service(create_primary_lang_badge)
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}

#[get("/")]
fn redirect_index() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .header(LOCATION, "https://github.com/XAMPPRocky/tokei")
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
            .header(CACHE_CONTROL, CacheControl(vec![CacheDirective::NoCache]))
            .header(ETAG, EntityTag::new(false, $etag))
            .header(
                CONTENT_TYPE,
                if $accept == ContentType::json() {
                    ContentType::json()
                } else {
                    CONTENT_TYPE_SVG.clone()
                },
            )
            .body($body)
    }};
}

#[derive(serde::Deserialize)]
struct BadgeQuery {
    category: Option<String>,
}

#[get("/b1/{domain}/{user}/{repo}")]
async fn create_stats_badge(
    request: HttpRequest,
    web::Path((domain, user, repo)): web::Path<(String, String, String)>,
    web::Query(query): web::Query<BadgeQuery>,
) -> actix_web::Result<HttpResponse> {
    let category = query.category.unwrap_or(String::from("lines"));

    let content_type = if let Ok(accept) = Accept::parse(&request) {
        if accept == Accept::json() {
            ContentType::json()
        } else {
            CONTENT_TYPE_SVG.clone()
        }
    } else {
        CONTENT_TYPE_SVG.clone()
    };

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
            STATS_CACHE
                .lock()
                .unwrap()
                .cache_get(&repo_identifier(&url, &sha));
            log::info!("{}#{} Not Modified", url, sha);
            return Ok(respond!(NotModified));
        }
    }

    let entry = get_statistics(&url, &sha).map_err(|err| actix_web::error::ErrorBadRequest(err))?;

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

    let badge = make_stats_badge(&content_type, &stats, &category)?;

    Ok(respond!(Ok, content_type, badge, sha))
}

fn repo_identifier(url: &str, sha: &str) -> String {
    format!("{}#{}", url, sha)
}

#[cached::proc_macro::cached(
    name = "STATS_CACHE",
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

#[cached::proc_macro::cached(
    name = "PRIMARY_LANG_CACHE",
    result = true,
    with_cached_flag = true,
    type = "cached::TimedSizedCache<String, cached::Return<LanguageType>>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(1000, DAY_IN_SECONDS) }",
    convert = r#"{ repo_identifier(url, _sha) }"#
)]
fn get_primary_lang(url: &str, _sha: &str) -> eyre::Result<cached::Return<LanguageType>> {
    log::info!("{} - Cloning", url);
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args(&["clone", &url, &temp_path, "--depth", "1"])
        .output()?;

    let mut languages = Languages::new();
    log::info!("{} - Getting Statistics", url);
    languages.get_statistics(&[temp_path], &[], &tokei::Config::default());

    let (primary_language_type, _) = languages
        .iter()
        .max_by_key(|(_, lang)| lang.code)
        .expect("No primary language");

    Ok(cached::Return::new(*primary_language_type))
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

#[get("/b2/{domain}/{user}/{repo}")]
async fn create_primary_lang_badge(
    request: HttpRequest,
    web::Path((domain, user, repo)): web::Path<(String, String, String)>,
) -> actix_web::Result<HttpResponse> {
    let content_type = match Accept::parse(&request) {
        Ok(accept) if accept == Accept::json() => ContentType::json(),
        _ => CONTENT_TYPE_SVG.clone()
    };

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
            PRIMARY_LANG_CACHE
                .lock()
                .unwrap()
                .cache_get(&repo_identifier(&url, &sha));
            log::info!("{}#{} Not Modified", url, sha);
            return Ok(respond!(NotModified));
        }
    }

    let entry = get_primary_lang(&url, &sha).map_err(|err| actix_web::error::ErrorBadRequest(err))?;

    if entry.was_cached {
        log::info!("{}#{} Cache hit", url, sha);
    }

    let primary_language_type = entry.value;

    log::info!(
        "{url}#{sha} - Primary Language {lang}",
        url = url,
        sha = sha,
        lang = primary_language_type,
    );

    let badge = make_primary_lang_badge(&content_type, &primary_language_type)?;

    Ok(respond!(Ok, content_type, badge, sha))

}

fn make_stats_badge(
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

fn make_primary_lang_badge(content_type: &ContentType, lang: &LanguageType) -> actix_web::Result<String> {
    if *content_type == ContentType::json() {
        return Ok(serde_json::to_string(&lang)?);
    }

    let options = BadgeOptions {
        subject: String::from(PRIMARY_LANG),
        status: lang.to_string(),
        color: String::from(BLUE),
    };

    Ok(Badge::new(options).unwrap().to_svg())
}
// 
// fn process_domain<'a>(domain: &'a str) -> std::result::Result<Cow<'a, str>, Utf8Error> {
//     let domain = percent_encoding::percent_decode_str(domain).decode_utf8()?;
// 
//     // For backwards compatability if a domain isn't specified we append `.com`.
//     let domain = if domain.contains('.') {
//         domain
//     } else {
//         domain + ".com"
//     };
// 
//     Ok(domain)
// }
