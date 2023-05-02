use std::process::Command;

use actix_web::{
    get,
    http::header::{
        Accept, CacheControl, CacheDirective, ContentType, EntityTag, Header, IfNoneMatch,
        CACHE_CONTROL, CONTENT_TYPE, ETAG, LOCATION,
    },
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use cached::{Cached, Return};
use csscolorparser::parse;
use once_cell::sync::Lazy;
use rsbadges::{Badge, Style};
use std::collections::HashSet;
use tempfile::TempDir;
use tokei::{Language, LanguageType, Languages};

const BILLION: usize = 1_000_000_000;
const BLANKS: &str = "blank lines";
const BLUE: &str = "#007ec6";
const GREY: &str = "#555555";
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
    label: Option<String>,
    style: Option<String>,
    color: Option<String>,
    logo: Option<String>,
    r#type: Option<String>,
}

#[get("/b1/{domain}/{user}/{repo}")]
async fn create_badge(
    request: HttpRequest,
    path: web::Path<(String, String, String)>,
    web::Query(query): web::Query<BadgeQuery>,
) -> actix_web::Result<HttpResponse> {
    let (domain, user, repo) = path.into_inner();
    let category = query.category.unwrap_or_else(|| "lines".to_owned());
    let (label, no_label) = match query.label {
        Some(v) => (v, false),
        None => ("".to_owned(), true),
    };
    let style: String = query.style.unwrap_or_else(|| "plastic".to_owned());
    let color: String = query.color.unwrap_or_else(|| BLUE.to_owned());
    let logo: String = query.logo.unwrap_or_else(|| "".to_owned());
    let r#type: String = query.r#type.unwrap_or_else(|| "".to_owned());

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
        .map(|i| ls_remote.stdout[..i].to_owned())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;

    if let Ok(if_none_match) = IfNoneMatch::parse(&request) {
        log::debug!("Checking If-None-Match: {}", sha);
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

    let entry: Return<Vec<(LanguageType, Language)>> =
        get_statistics(&url, &sha).map_err(actix_web::error::ErrorBadRequest)?;

    if entry.was_cached {
        log::info!("{}#{} Cache hit", url, sha);
    }

    let language_types: HashSet<LanguageType> = r#type
        .split(',')
        .filter_map(|s| str::parse::<LanguageType>(s).ok())
        .into_iter()
        .collect::<HashSet<LanguageType>>();

    let mut stats = Language::new();
    let languages: Vec<(LanguageType, Language)> = entry.value;

    for (language_type, language) in &languages {
        if language_types.is_empty() || language_types.contains(&language_type) {
            stats += language.clone();
        }
    }

    log::info!(
        "{url}#{sha} - Languages (most common to least common) {languages:#?} Lines {lines} Code {code} Comments {comments} Blanks {blanks}",
        url = url,
        sha = sha,
        languages = languages,
        lines = stats.lines(),
        code = stats.code,
        comments = stats.comments,
        blanks = stats.blanks
    );

    let badge = make_badge(
        &content_type,
        &stats,
        &category,
        &label,
        &style,
        &color,
        &logo,
        no_label,
    )?;

    Ok(respond!(Ok, content_type, badge, sha))
}

fn repo_identifier(url: &str, sha: &str) -> String {
    format!("{}#{}", url, sha)
}

#[cached::proc_macro::cached(
    name = "CACHE",
    result = true,
    with_cached_flag = true,
    type = "cached::TimedSizedCache<String, cached::Return<Vec<(LanguageType,Language)>>>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(1000, DAY_IN_SECONDS) }",
    convert = r#"{ repo_identifier(url, _sha) }"#
)]
fn get_statistics(
    url: &str,
    _sha: &str,
) -> eyre::Result<cached::Return<Vec<(LanguageType, Language)>>> {
    log::info!("{} - Cloning", url);
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args(["clone", url, temp_path, "--depth", "1"])
        .output()?;

    let mut languages = Languages::new();
    log::info!("{} - Getting Statistics", url);
    languages.get_statistics(&[temp_path], &[], &tokei::Config::default());

    let mut iter = languages.iter_mut();
    while let Some((_, language)) = iter.next() {
        for report in &mut language.reports {
            report.name = report.name.strip_prefix(temp_path)?.to_owned();
        }
    }

    Ok(cached::Return::new(languages.into_iter().collect()))
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

fn make_badge_style(
    label: &str,
    amount: &str,
    color: &str,
    style: &str,
    logo: &str,
) -> Result<String, actix_web::Error> {
    fn badge(label: &str, amount: &str, color: &str) -> Badge {
        Badge {
            label_text: label.to_owned(),
            label_color: GREY.to_owned(),
            msg_text: amount.to_owned(),
            msg_color: match parse(color) {
                Ok(result) => result.to_hex_string(),
                Err(_error) => BLUE.to_owned(),
            },
            ..Badge::default()
        }
    }

    let badge_with_logo = Badge {
        logo: logo.to_owned(),
        embed_logo: !logo.is_empty(),
        ..badge(label, amount, color)
    };

    fn stylize_badge(badge: Badge, style: &str) -> Style {
        match style {
            "flat" => Style::Flat(badge),
            "flat-square" => Style::FlatSquare(badge),
            "plastic" => Style::Plastic(badge),
            "for-the-badge" => Style::ForTheBadge(badge),
            "social" => Style::Social(badge),
            _ => Style::Flat(badge),
        }
    }

    match stylize_badge(badge_with_logo, style).generate_svg() {
        Ok(s) => Ok(s),
        Err(_e) => Ok(stylize_badge(badge(label, amount, color), style)
            .generate_svg()
            .unwrap()),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_badge(
    content_type: &ContentType,
    stats: &Language,
    category: &str,
    label: &str,
    style: &str,
    color: &str,
    logo: &str,
    no_label: bool,
) -> actix_web::Result<String> {
    if *content_type == ContentType::json() {
        return Ok(serde_json::to_string(&stats)?);
    }

    let (amount, label) = match category {
        "code" => (stats.code, if no_label { CODE } else { label }),
        "files" => (stats.reports.len(), if no_label { FILES } else { label }),
        "blanks" => (stats.blanks, if no_label { BLANKS } else { label }),
        "comments" => (stats.comments, if no_label { COMMENTS } else { label }),
        _ => (stats.lines(), if no_label { LINES } else { label }),
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

    make_badge_style(label, &amount, color, style, logo)
}
