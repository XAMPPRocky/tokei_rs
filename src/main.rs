#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate lazy_static;
extern crate badge;
extern crate dotenv;
#[macro_use]
extern crate dotenv_codegen;
extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;
#[macro_use]
extern crate rocket;
extern crate tempdir;
extern crate tokei;

use std::io::{self, Cursor};
use std::process::Command;

use badge::{Badge, BadgeOptions};
use r2d2_postgres::{PostgresConnectionManager, TlsMode};
use rocket::http::{ContentType, Status};
use rocket::request::Form;
use rocket::response::Redirect;
use rocket::{Response, State};
use tempdir::TempDir;
use tokei::{Language, Languages};

const BILLION: i64 = 1_000_000_000;
const MILLION: i64 = 1_000_000;
const THOUSAND: i64 = 1_000;

const BLANKS: &'static str = "Blank lines";
const BLUE: &'static str = "#007ec6";
const CODE: &'static str = "LoC";
const COMMENTS: &'static str = "Comments";
const FILES: &'static str = "Files";
const LINES: &'static str = "Total lines";
const RED: &'static str = "#e05d44";
const INSERT: &'static str = r#"
INSERT INTO repo (hash, code, lines, blanks, files, comments)
VALUES ($1, $2, $3, $4, $5, $6)
"#;

#[derive(FromForm)]
struct Category {
    pub category: String,
}

impl Default for Category {
    fn default() -> Self {
        Category {
            category: String::from("code"),
        }
    }
}

lazy_static! {
    static ref ERROR_BADGE: String = {
        let options = BadgeOptions {
            subject: String::from("Error"),
            status: String::from("Url incorrect"),
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

    ($status:expr, $body:expr, $etag:expr) => {{
        use rocket::http::hyper::header::{CacheControl, CacheDirective, ETag, EntityTag};

        let mut response = Response::new();
        response.set_status($status);
        response.set_sized_body(Cursor::new($body));
        response.set_header(ContentType::SVG);
        response.set_header(CacheControl(vec![CacheDirective::NoCache]));
        response.set_header(ETag(EntityTag::new(false, $etag)));
        Ok(response)
    }};
}

fn main() {
    let manager = PostgresConnectionManager::new(dotenv!("POSTGRES_URL"), TlsMode::None).unwrap();

    let pool = r2d2::Pool::new(manager).unwrap();

    rocket::ignite()
        .manage(pool)
        .mount("/", routes![index, badge, badge_no_args])
        .launch();
}

#[get("/")]
fn index() -> Redirect {
    Redirect::permanent("https://github.com/Aaronepower/tokei")
}

#[get("/b1/<domain>/<user>/<repo>")]
fn badge_no_args(
    domain: String,
    user: String,
    repo: String,
    pool: State<r2d2::Pool<PostgresConnectionManager>>,
) -> io::Result<Response> {
    badge(domain, user, repo, None, pool)
}

#[get("/b1/<domain>/<user>/<repo>?<category..>")]
fn badge<'a, 'b>(
    domain: String,
    user: String,
    repo: String,
    category: Option<Form<Category>>,
    pool: State<r2d2::Pool<PostgresConnectionManager>>,
) -> io::Result<Response<'b>> {
    let conn = pool.get().unwrap();
    let category = category
        .map(|c| c.0)
        .unwrap_or(Category::default())
        .category;
    println!("{}", category);

    let url = format!("https://{}.com/{}/{}", domain, user, repo);

    let (select_query, text) = match &*category {
        "code" => ("SELECT code FROM repo WHERE hash = $1", CODE),
        "blanks" => ("SELECT blanks FROM repo WHERE hash = $1", BLANKS),
        "files" => ("SELECT files FROM repo WHERE hash = $1", FILES),
        "lines" => ("SELECT lines FROM repo WHERE hash = $1", LINES),
        "comments" => ("SELECT comments FROM repo WHERE hash = $1", COMMENTS),
        _ => return respond!(Status::BadRequest, &**ERROR_BADGE),
    };

    let ls_remote = Command::new("git").arg("ls-remote").arg(&url).output()?;

    let stdout = ls_remote.stdout;
    let end_of_sha = stdout.iter().position(|&b| b == b'\t').unwrap_or(0);
    let hash = String::from_utf8_lossy(&stdout[..end_of_sha]);
    let select = conn.prepare_cached(select_query)?;
    let rows = select.query(&[&&*hash])?;
    let temp_dir = TempDir::new("repo")?;
    let temp_path = temp_dir.path().to_str().unwrap();

    for row in &rows {
        let stat: i64 = row.get(0);
        return respond!(Status::Ok, make_badge(stat, text), (&*hash).to_owned());
    }

    Command::new("git")
        .args(&["clone", &url, &temp_path, "--depth", "1"])
        .output()?;

    let mut languages = Languages::new();
    languages.get_statistics(vec![temp_path], Vec::new());

    let mut stats = Language::new_blank();

    for (_, language) in languages {
        stats += language;
    }

    let insert = conn.prepare_cached(INSERT).unwrap();
    insert.execute(&[
        &&*hash,
        &(stats.code as i64),
        &(stats.lines as i64),
        &(stats.blanks as i64),
        &(stats.stats.len() as i64),
        &(stats.comments as i64),
    ])?;

    let stat = match &*category {
        "code" => stats.code,
        "lines" => stats.lines,
        "blanks" => stats.blanks,
        "files" => stats.stats.len(),
        "comments" => stats.comments,
        _ => unreachable!(),
    } as i64;

    respond!(Status::Ok, make_badge(stat, text), (&*hash).to_owned())
}

fn trim_and_float(num: i64, trim: i64) -> f64 {
    num as f64 / trim as f64
}

fn make_badge(amount: i64, category: &str) -> String {
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

    Badge::new(options).unwrap().to_svg()
}
