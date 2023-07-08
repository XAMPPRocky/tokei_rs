use std::process::{Command, Output};

use actix_web::{
    get,
    http::header::{
        Accept, CacheControl, CacheDirective, ContentType, EntityTag, Header, IfNoneMatch,
        CACHE_CONTROL, CONTENT_TYPE, ETAG, LOCATION,
    },
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer,
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

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
struct BadgeQuery {
    category: Option<String>,
    label: Option<String>,
    style: Option<String>,
    color: Option<String>,
    logo: Option<String>,
    r#type: Option<String>,
    showLanguage: Option<String>,
    languageRank: Option<String>,
    branch: Option<String>,
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
    let show_language: bool = query
        .showLanguage
        .unwrap_or_else(|| "".to_owned())
        .parse::<bool>()
        .unwrap_or(false);
    let language_rank: usize = match query.languageRank {
        Some(s) => s.parse::<usize>().unwrap_or(0),
        None => 1,
    };
    let branch: String = query.branch.unwrap_or_else(|| "".to_owned());

    let content_type: ContentType = if let Ok(accept) = Accept::parse(&request) {
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

    let url: &str = &format!("https://{}/{}/{}", domain, user, repo);

    let ls_remote: Output = Command::new("git")
        .args(["ls-remote", "--symref", url, "HEAD", "refs/heads/**"])
        .output()?;

    let ls_remote_output: String = String::from_utf8(ls_remote.stdout)
        .ok()
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;
    (!ls_remote_output.is_empty())
        .then(|| ())
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;

    let git_lines: Vec<&str> = ls_remote_output.split("\n").collect();
    (git_lines.len() > 1)
        .then(|| ())
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;

    let mut iter = git_lines.iter();
    let head_branch: &str = match iter.next() {
        Some(&s) => {
            let without_prefix: &str = match s.strip_prefix("ref: refs/heads/") {
                Some(b) => b,
                None => "",
            };
            let without_prefix_and_suffix: &str = match without_prefix.strip_suffix("\tHEAD") {
                Some(c) => c,
                None => "",
            };
            without_prefix_and_suffix
        }
        None => "",
    };
    iter.next(); // skip 2nd line with HEAD
    let branch_name: &str = if branch.is_empty() {
        head_branch
    } else {
        &branch
    };
    let mut sha: &str = "";
    while let Some(&line) = iter.next() {
        let (s, bn) = match line.split_once("\trefs/heads/") {
            Some((s, bn)) => (s, bn),
            None => ("", ""),
        };
        if bn == branch_name {
            sha = s;
            break;
        }
    }
    (sha.len() == HASH_LENGTH)
        .then(|| ())
        .ok_or_else(|| actix_web::error::ErrorBadRequest(eyre::eyre!("Invalid SHA provided.")))?;

    if let Ok(if_none_match) = IfNoneMatch::parse(&request) {
        log::debug!("Checking If-None-Match: {}#{}", sha, branch_name);
        let entity_tag: EntityTag = EntityTag::new(false, etag_identifier(sha, branch_name));
        let found_match: bool = match if_none_match {
            IfNoneMatch::Any => false,
            IfNoneMatch::Items(items) => items
                .iter()
                .any(|etag: &EntityTag| etag.weak_eq(&entity_tag)),
        };

        if found_match {
            CACHE
                .lock()
                .unwrap()
                .cache_get(&repo_identifier(&url, sha, branch_name));
            log::info!("{}#{}#{} Not Modified", url, sha, branch_name);
            return Ok(respond!(NotModified));
        }
    }

    let entry: Return<Vec<(LanguageType, Language)>> =
        get_statistics(&url, &sha, &branch_name).map_err(actix_web::error::ErrorBadRequest)?;

    if entry.was_cached {
        log::info!("{}#{}#{} Cache hit", url, sha, branch_name);
    }

    let language_types: HashSet<LanguageType> = r#type
        .split(',')
        .filter_map(|s: &str| str::parse::<LanguageType>(s).ok())
        .into_iter()
        .collect::<HashSet<LanguageType>>();

    let languages: Vec<(LanguageType, Language)> = if language_types.is_empty() {
        entry.value
    } else {
        entry
            .value
            .into_iter()
            .filter(|(language_type, _)| language_types.contains(&language_type))
            .into_iter()
            .collect()
    };
    let ranking_language = if !show_language {
        String::new()
    } else if languages.is_empty() {
        "No Languages".to_owned()
    } else if language_rank == 0 || language_rank > languages.len() {
        "N/A".to_owned()
    } else {
        let (ranking_language_type, _) = languages[language_rank - 1];
        ranking_language_type.name().to_owned()
    };

    let mut stats = Language::new();
    for (_, language) in &languages {
        stats += language.clone();
    }

    log::info!(
        "{url}#{sha}#{branch_name} - Languages (most common to least common) {languages:#?} Lines {lines} Code {code} Comments {comments} Blanks {blanks}",
        url = url,
        sha = sha,
        branch_name = branch_name,
        languages = languages,
        lines = stats.lines(),
        code = stats.code,
        comments = stats.comments,
        blanks = stats.blanks
    );

    let badge: String = make_badge(
        &content_type,
        &stats,
        &category,
        &label,
        &style,
        &color,
        &logo,
        &ranking_language,
        no_label,
    )
    .await?;

    Ok(respond!(
        Ok,
        content_type,
        badge,
        etag_identifier(sha, branch_name)
    ))
}

fn repo_identifier(url: &str, sha: &str, branch_name: &str) -> String {
    format!("{}#{}#{}", url, sha, branch_name)
}

fn etag_identifier(sha: &str, branch_name: &str) -> String {
    format!("{}#{}", sha, branch_name)
}

#[cached::proc_macro::cached(
    name = "CACHE",
    result = true,
    with_cached_flag = true,
    type = "cached::TimedSizedCache<String, cached::Return<Vec<(LanguageType,Language)>>>",
    create = "{ cached::TimedSizedCache::with_size_and_lifespan(1000, DAY_IN_SECONDS) }",
    convert = r#"{ repo_identifier(url, _sha, branch_name) }"#
)]
fn get_statistics(
    url: &str,
    _sha: &str,
    branch_name: &str,
) -> eyre::Result<cached::Return<Vec<(LanguageType, Language)>>> {
    log::info!("{} - Cloning", url);
    let temp_dir: TempDir = TempDir::new()?;
    let temp_path: &str = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args([
            "clone",
            url,
            temp_path,
            "--depth",
            "1",
            "--branch",
            branch_name,
        ])
        .output()?;

    let mut languages: Languages = Languages::new();
    log::info!("{} - Getting Statistics", url);
    languages.get_statistics(&[temp_path], &[], &tokei::Config::default());

    let mut iter = languages.iter_mut();
    while let Some((_, language)) = iter.next() {
        for report in &mut language.reports {
            report.name = report.name.strip_prefix(temp_path)?.to_owned();
        }
        for (_, child) in &mut language.children {
            for language in child.into_iter() {
                language.name = language.name.strip_prefix(temp_path)?.to_owned();
            }
        }
    }

    let mut languages_sorted_by_lines_of_code: Vec<(LanguageType, Language)> =
        languages.into_iter().collect();
    languages_sorted_by_lines_of_code.sort_by(|(_, a), (_, b)| b.code.cmp(&a.code));

    Ok(cached::Return::new(languages_sorted_by_lines_of_code))
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

async fn make_badge_style(
    label: &str,
    msg: &str,
    color: &str,
    style: &str,
    logo: &str,
) -> Result<String, actix_web::Error> {
    fn badge(label: &str, msg: &str, color: &str) -> Badge {
        Badge {
            label_text: label.to_owned(),
            label_color: GREY.to_owned(),
            msg_text: msg.to_owned(),
            msg_color: match parse(color) {
                Ok(result) => result.to_hex_string(),
                Err(_error) => BLUE.to_owned(),
            },
            ..Badge::default()
        }
    }

    let badge_with_logo: Badge = Badge {
        logo: logo.to_owned(),
        embed_logo: !logo.is_empty(),
        ..badge(label, msg, color)
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
        Err(_e) => Ok(stylize_badge(badge(label, msg, color), style)
            .generate_svg()
            .unwrap()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn make_badge(
    content_type: &ContentType,
    stats: &Language,
    category: &str,
    label: &str,
    style: &str,
    color: &str,
    logo: &str,
    ranking_language: &str,
    no_label: bool,
) -> actix_web::Result<String> {
    if *content_type == ContentType::json() {
        return Ok(serde_json::to_string(&stats)?);
    }

    if !ranking_language.is_empty() {
        return make_badge_style(label, ranking_language, color, style, logo).await;
    }

    let (amount, label) = match category {
        "code" => (stats.code, if no_label { CODE } else { label }),
        "files" => (stats.reports.len(), if no_label { FILES } else { label }),
        "blanks" => (stats.blanks, if no_label { BLANKS } else { label }),
        "comments" => (stats.comments, if no_label { COMMENTS } else { label }),
        _ => (stats.lines(), if no_label { LINES } else { label }),
    };

    let amount: String = if amount >= BILLION {
        format!("{:.1}B", trim_and_float(amount, BILLION))
    } else if amount >= MILLION {
        format!("{:.1}M", trim_and_float(amount, MILLION))
    } else if amount >= THOUSAND {
        format!("{:.1}K", trim_and_float(amount, THOUSAND))
    } else {
        amount.to_string()
    };

    make_badge_style(label, &amount, color, style, logo).await
}
