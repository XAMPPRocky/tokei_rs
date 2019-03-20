#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;

use std::{env, io::{self, Cursor}, path::PathBuf, process::Command};

use badge::{Badge, BadgeOptions};
use lazy_static::lazy_static;
use r2d2_postgres::{TlsMode, PostgresConnectionManager};
use rocket::{
    Response,
    State,
    http::{Accept, ContentType, Status},
    response::Redirect,
};
use tempfile::TempDir;
use tokei::{Language, Languages, Stats};

type Result<T> = std::result::Result<T, failure::Error>;

const SELECT_STATS: &str = "
SELECT blanks, code, comments, lines FROM repo WHERE hash = $1";
const SELECT_FILES: &str = "
SELECT name, blanks, code, comments, lines FROM stats WHERE hash = $1";
const INSERT_STATS: &'static str = r#"
INSERT INTO repo (hash, blanks, code, comments, lines)
VALUES ($1, $2, $3, $4, $5)
"#;

const INSERT_FILES: &'static str = r#"
INSERT INTO stats (hash, blanks, code, comments, lines, name)
VALUES ($1, $2, $3, $4, $5, $6)
"#;


const BILLION: usize = 1_000_000_000;
const MILLION: usize = 1_000_000;
const THOUSAND: usize = 1_000;

const BLANKS: &'static str = "Blank lines";
const BLUE: &'static str = "#007ec6";
const CODE: &'static str = "Loc";
const COMMENTS: &'static str = "Comments";
const FILES: &'static str = "Files";
const LINES: &'static str = "Total lines";
const RED: &'static str = "#e05d44";
lazy_static! {
    static ref ERROR_BADGE: String = {
        let options = BadgeOptions {
            subject: String::from("Error"),
            status: String::from("Incorrect URL"),
            color: String::from(RED),
        };

        Badge::new(options).unwrap().to_svg()
    };
}

macro_rules! respond {
    ($status:expr, $body:expr) => {{

        let mut response = Response::new();
        response.set_status($status);
        response.set_sized_body(Cursor::new($body));
        response.set_header(ContentType::SVG);
        Ok(response)
    }};

    ($status:expr, $accept:expr, $body:expr, $etag:expr) => {{
        use rocket::http::hyper::header::{
            CacheControl,
            CacheDirective,
            EntityTag,
            ETag
        };

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
    }}
}

fn main() {
    dotenv::dotenv().unwrap();
    let manager = PostgresConnectionManager::new(
        env::var("POSTGRES_URL").unwrap(),
        TlsMode::None
    ).unwrap();

    let pool = r2d2::Pool::new(manager).unwrap();

    rocket::ignite()
        .manage(pool)
        .mount("/", routes![index, badge])
        .launch();
}

#[get("/")]
fn index() -> Redirect {
    Redirect::permanent("https://github.com/Aaronepower/tokei")
}

#[get("/b1/<domain>/<user>/<repo>?<category>")]
fn badge<'a, 'b>(accept_header: &Accept,
                 domain: String,
                 user: String,
                 repo: String,
                 category: Option<String>,
                 pool: State<r2d2::Pool<PostgresConnectionManager>>)
    -> Result<Response<'b>>
{
    let conn = pool.get().unwrap();
    let category = match category.as_ref().map(|x| &**x) {
        Some("code") => CODE,
        Some("blanks") => BLANKS,
        Some("comments") => COMMENTS,
        _ => LINES,
    };
    let url = format!("https://{}.com/{}/{}", domain, user, repo);
    let ls_remote = Command::new("git").arg("ls-remote").arg(&url).output()?;
    let stdout = ls_remote.stdout;
    let end_of_sha = match stdout.iter().position(|&b| b == b'\t') {
        Some(index) => index,
        None => return respond!(Status::BadRequest, &**ERROR_BADGE),
    };
    let hash = String::from_utf8_lossy(&stdout[..end_of_sha]);
    let select = conn.prepare_cached(SELECT_STATS)?;
    let rows = select.query(&[&&*hash])?;

    if let Some(row) = rows.iter().next() {
        let mut stats: Language = Language::new();

        let select_files = conn.prepare_cached(SELECT_FILES)?;

        for row in select_files.query(&[&&*hash])?.iter() {
            let mut stat = Stats::new(PathBuf::from(row.get::<_, String>(0)));
            stat.blanks = row.get::<_, i64>(1) as usize;
            stat.code = row.get::<_, i64>(2) as usize;
            stat.comments = row.get::<_, i64>(3) as usize;
            stat.lines = row.get::<_, i64>(4) as usize;

            stats.stats.push(stat);
        }

        stats.blanks = row.get::<_, i64>(0) as usize;
        stats.code = row.get::<_, i64>(1) as usize;
        stats.comments = row.get::<_, i64>(2) as usize;
        stats.lines = row.get::<_, i64>(3) as usize;

        return respond!(Status::Ok, accept_header, make_badge(accept_header, stats, category)?, (&*hash).to_owned())
    }

    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_str().unwrap();

    Command::new("git")
        .args(&["clone", &url, &temp_path, "--depth", "1"])
        .output()?;

    let mut stats = Language::new();
    let mut languages = Languages::new();
    languages.get_statistics(&[temp_path], &[], &tokei::Config::default());

    for (_, language) in languages {
        stats += language;
    }

    let insert_files = conn.prepare_cached(INSERT_FILES)?;

    for stat in &mut stats.stats {
        stat.name = stat.name.strip_prefix(temp_path)?.to_owned();
        insert_files.execute(&[
            &&*hash,
            &(stat.blanks as i64),
            &(stat.code as i64),
            &(stat.comments as i64),
            &(stat.lines as i64),
            &stat.name.to_str().unwrap(),
        ])?;
    }

    let insert_stats = conn.prepare_cached(INSERT_STATS).unwrap();
    insert_stats.execute(&[
        &&*hash,
        &(stats.blanks as i64),
        &(stats.code as i64),
        &(stats.comments as i64),
        &(stats.lines as i64),
    ])?;

    respond!(Status::Ok,
             accept_header,
             make_badge(accept_header, stats, category)?,
             (&*hash).to_owned())
}

fn trim_and_float(num: usize, trim: usize) -> f64 {
    (num as f64) / (trim as f64)
}

fn make_badge(accept: &Accept, stats: Language, category: &str)
    -> Result<String>
{
    if *accept == Accept::JSON {
        return Ok(serde_json::to_string(&stats)?)
    }

    let amount = match &*category {
        "code" => stats.code,
        "files" => stats.stats.len(),
        "blanks" => stats.blanks,
        "comments" => stats.comments,
        _ => stats.lines,
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
        subject: String::from(category),
        status: amount,
        color: String::from(BLUE),
    };

    Ok(Badge::new(options).unwrap().to_svg())
}
