//! Command dispatch: one API call per command, then print.

use std::io::BufRead;

use mandrake_core::{
    Id,
    api::{AuditEntry, Job, Page, Session, SystemInfo, SystemResources, Token, TokenCreated, User},
};
use serde_json::{Value, json};

use crate::{
    cli::{AuditCmd, Cli, Command, JobsCmd, Paging, PasswordArgs, SystemCmd, TokensCmd, UsersCmd},
    client::{Client, Error},
    output,
};

/// Run the command; the bool says whether JSON was requested.
pub async fn run(cli: Cli) -> Result<(), Error> {
    let json = output::json_wanted(cli.json);
    let client = Client::connect(&cli).await?;
    match cli.command {
        Command::Health => {
            client.empty("GET", "/health", None).await?;
            if json {
                output::json(&json!({ "ok": true }));
            } else {
                println!("ok");
            }
        }
        Command::Session => {
            let s: Session = client.json("GET", "/auth/session", &[], None).await?;
            if json {
                output::json(&serde_json::to_value(&s)?);
            } else {
                output::kv(&[
                    ("username", s.actor.username.clone()),
                    ("role", s.actor.role.to_string()),
                    ("via", format!("{:?}", s.actor.via).to_lowercase()),
                    (
                        "user id",
                        output::opt(s.actor.id.map(|i| i.to_string()).as_deref()),
                    ),
                    (
                        "token id",
                        output::opt(s.actor.token_id.map(|i| i.to_string()).as_deref()),
                    ),
                    ("expires", output::ts(s.expires_at)),
                    ("idle expires", output::ts(s.idle_expires_at)),
                ]);
            }
        }
        Command::System(cmd) => system(&client, cmd, json).await?,
        Command::Users(cmd) => users(&client, cmd, json).await?,
        Command::Tokens(cmd) => tokens(&client, cmd, json).await?,
        Command::Audit(cmd) => audit(&client, cmd, json).await?,
        Command::Jobs(cmd) => jobs(&client, cmd, json).await?,
        Command::Storage(cmd) => crate::storage::run(&client, cmd, json).await?,
        Command::Network(cmd) => crate::network::run(&client, cmd, json).await?,
        Command::Images(cmd) => crate::images::run(&client, cmd, json).await?,
        Command::Zones(cmd) => crate::zones::run(&client, cmd, json).await?,
        Command::Vms(cmd) => crate::vms::run(&client, cmd, json).await?,
    }
    Ok(())
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Unexpected {
            status: 0,
            body: e.to_string(),
        }
    }
}

/// Fetch one page, or every page with `--all`.
pub(crate) async fn pages<T: serde::de::DeserializeOwned>(
    client: &Client,
    path: &str,
    base_query: &[(&str, String)],
    paging: &Paging,
) -> Result<Page<T>, Error> {
    let mut items = Vec::new();
    let mut cursor = paging.cursor.clone();
    loop {
        let mut query: Vec<(&str, String)> = base_query.to_vec();
        if let Some(limit) = paging.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(c) = &cursor {
            query.push(("cursor", c.clone()));
        }
        let page: Page<T> = client.json("GET", path, &query, None).await?;
        items.extend(page.items);
        cursor = page.next_cursor;
        if cursor.is_none() || !paging.all {
            break;
        }
    }
    Ok(Page {
        items,
        next_cursor: cursor,
    })
}

fn read_stdin_lines(n: usize) -> Result<Vec<String>, Error> {
    let stdin = std::io::stdin();
    let mut lines = Vec::new();
    for line in stdin.lock().lines().take(n) {
        lines.push(line.map_err(|e| Error::Config(format!("reading stdin: {e}")))?);
    }
    Ok(lines)
}

fn password_from(args: &PasswordArgs, stdin_lines: &[String]) -> Result<String, Error> {
    if let Some(p) = &args.password {
        return Ok(p.clone());
    }
    if args.password_stdin {
        return stdin_lines
            .first()
            .cloned()
            .ok_or_else(|| Error::Config("no password on stdin".to_owned()));
    }
    Err(Error::Config(
        "give the password with --password, MANDRAKE_PASSWORD, or --password-stdin".to_owned(),
    ))
}

fn user_row(u: &User) -> Vec<String> {
    vec![
        u.id.to_string(),
        u.username.clone(),
        u.role.to_string(),
        output::opt(u.display_name.as_deref()),
        if u.disabled {
            "disabled".to_owned()
        } else if u.locked_until.is_some() {
            "locked".to_owned()
        } else {
            "active".to_owned()
        },
        output::ts(u.last_login_at),
    ]
}

const USER_HEADERS: [&str; 6] = [
    "ID",
    "USERNAME",
    "ROLE",
    "DISPLAY NAME",
    "STATE",
    "LAST LOGIN",
];

fn print_user(u: &User, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(u)?);
    } else {
        output::table(&USER_HEADERS, &[user_row(u)]);
    }
    Ok(())
}

async fn system(client: &Client, cmd: SystemCmd, json: bool) -> Result<(), Error> {
    match cmd {
        SystemCmd::Info => {
            let s: SystemInfo = client.json("GET", "/system", &[], None).await?;
            if json {
                output::json(&serde_json::to_value(&s)?);
            } else {
                output::kv(&[
                    ("hostname", s.hostname.clone()),
                    ("product", format!("{} {}", s.product, s.version)),
                    ("omnios", s.omnios_release.clone()),
                    ("boot environment", s.boot_environment.clone()),
                    ("uptime", output::duration(s.uptime_seconds)),
                    ("time", s.time.to_rfc3339()),
                    ("timezone", output::opt(s.timezone.as_deref())),
                    ("host id", s.id.to_string()),
                ]);
            }
        }
        SystemCmd::Resources => {
            let r: SystemResources = client.json("GET", "/system/resources", &[], None).await?;
            if json {
                output::json(&serde_json::to_value(&r)?);
            } else {
                output::kv(&[
                    ("cpus", r.cpus.to_string()),
                    (
                        "load",
                        format!(
                            "{:.2} {:.2} {:.2}",
                            r.load_avg[0], r.load_avg[1], r.load_avg[2]
                        ),
                    ),
                    ("memory total", output::size(r.memory.total_bytes)),
                    ("memory free", output::size(r.memory.free_bytes)),
                    ("sampled", output::ts(r.sampled_at)),
                ]);
            }
        }
    }
    Ok(())
}

async fn users(client: &Client, cmd: UsersCmd, json: bool) -> Result<(), Error> {
    match cmd {
        UsersCmd::List { paging } => {
            let page: Page<User> = pages(client, "/users", &[], &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(user_row).collect();
                output::table(&USER_HEADERS, &rows);
                if let Some(c) = page.next_cursor {
                    eprintln!("more: --cursor {c}");
                }
            }
        }
        UsersCmd::Get { id } => {
            let u: User = client
                .json("GET", &format!("/users/{id}"), &[], None)
                .await?;
            print_user(&u, json)?;
        }
        UsersCmd::Create {
            username,
            role,
            display_name,
            password,
        } => {
            let lines = if password.password_stdin {
                read_stdin_lines(1)?
            } else {
                Vec::new()
            };
            let body = json!({
                "username": username,
                "password": password_from(&password, &lines)?,
                "role": role,
                "display_name": display_name,
            });
            let u: User = client.json("POST", "/users", &[], Some(&body)).await?;
            print_user(&u, json)?;
        }
        UsersCmd::Update {
            id,
            role,
            display_name,
            disable,
            enable,
        } => {
            let mut body = serde_json::Map::new();
            if let Some(r) = role {
                body.insert("role".to_owned(), serde_json::to_value(r)?);
            }
            if let Some(d) = display_name {
                body.insert("display_name".to_owned(), Value::String(d));
            }
            if disable || enable {
                body.insert("disabled".to_owned(), Value::Bool(disable));
            }
            if body.is_empty() {
                return Err(Error::Config("nothing to update".to_owned()));
            }
            let u: User = client
                .json(
                    "PATCH",
                    &format!("/users/{id}"),
                    &[],
                    Some(&Value::Object(body)),
                )
                .await?;
            print_user(&u, json)?;
        }
        UsersCmd::Delete { id } => {
            client
                .empty("DELETE", &format!("/users/{id}"), None)
                .await?;
            done(json, "deleted", id);
        }
        UsersCmd::Passwd {
            id,
            password,
            current,
            current_stdin,
        } => passwd(client, id, &password, current, current_stdin, json).await?,
    }
    Ok(())
}

async fn passwd(
    client: &Client,
    id: Id,
    password: &PasswordArgs,
    current: Option<String>,
    current_stdin: bool,
    json: bool,
) -> Result<(), Error> {
    let wanted = usize::from(password.password_stdin) + usize::from(current_stdin);
    let lines = if wanted > 0 {
        read_stdin_lines(wanted)?
    } else {
        Vec::new()
    };
    let new_password = password_from(password, &lines)?;
    let current = if current_stdin {
        Some(
            lines
                .get(usize::from(password.password_stdin))
                .cloned()
                .ok_or_else(|| Error::Config("no current password on stdin".to_owned()))?,
        )
    } else {
        current
    };
    let body = json!({ "current_password": current, "new_password": new_password });
    client
        .empty("PUT", &format!("/users/{id}/password"), Some(&body))
        .await?;
    done(json, "password changed", id);
    Ok(())
}

fn token_row(t: &Token) -> Vec<String> {
    vec![
        t.id.to_string(),
        t.name.clone(),
        format!("mdk_{}...", t.prefix),
        t.user_id.to_string(),
        output::ts(Some(t.created_at)),
        output::ts(t.expires_at),
        output::ts(t.last_used_at),
    ]
}

const TOKEN_HEADERS: [&str; 7] = [
    "ID",
    "NAME",
    "PREFIX",
    "USER",
    "CREATED",
    "EXPIRES",
    "LAST USED",
];

async fn tokens(client: &Client, cmd: TokensCmd, json: bool) -> Result<(), Error> {
    match cmd {
        TokensCmd::List { user, paging } => {
            let mut query = Vec::new();
            if let Some(u) = user {
                query.push(("user_id", u.to_string()));
            }
            let page: Page<Token> = pages(client, "/tokens", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(token_row).collect();
                output::table(&TOKEN_HEADERS, &rows);
            }
        }
        TokensCmd::Get { id } => {
            let t: Token = client
                .json("GET", &format!("/tokens/{id}"), &[], None)
                .await?;
            if json {
                output::json(&serde_json::to_value(&t)?);
            } else {
                output::table(&TOKEN_HEADERS, &[token_row(&t)]);
            }
        }
        TokensCmd::Create {
            name,
            user,
            expires_in,
        } => {
            let body = json!({ "name": name, "user_id": user, "expires_in_seconds": expires_in });
            let t: TokenCreated = client.json("POST", "/tokens", &[], Some(&body)).await?;
            if json {
                output::json(&serde_json::to_value(&t)?);
            } else {
                output::table(&TOKEN_HEADERS, &[token_row(&t.token)]);
                println!();
                println!("secret (shown once): {}", t.secret);
            }
        }
        TokensCmd::Revoke { id } => {
            client
                .empty("DELETE", &format!("/tokens/{id}"), None)
                .await?;
            done(json, "revoked", id);
        }
    }
    Ok(())
}

async fn audit(client: &Client, cmd: AuditCmd, json: bool) -> Result<(), Error> {
    let AuditCmd::List {
        action,
        actor,
        object,
        since,
        until,
        paging,
    } = cmd;
    let mut query = Vec::new();
    if let Some(a) = action {
        query.push(("action", a));
    }
    if let Some(a) = actor {
        query.push(("actor_id", a.to_string()));
    }
    if let Some(o) = object {
        query.push(("object_id", o.to_string()));
    }
    if let Some(s) = since {
        query.push(("since", s.to_rfc3339()));
    }
    if let Some(u) = until {
        query.push(("until", u.to_rfc3339()));
    }
    let page: Page<AuditEntry> = pages(client, "/audit", &query, &paging).await?;
    if json {
        output::json(&serde_json::to_value(&page)?);
        return Ok(());
    }
    let rows: Vec<Vec<String>> = page
        .items
        .iter()
        .map(|e| {
            vec![
                e.id.clone(),
                e.at.to_rfc3339(),
                format!(
                    "{} ({})",
                    e.actor.username,
                    format!("{:?}", e.actor.via).to_lowercase()
                ),
                e.action.clone(),
                match (&e.object.name, e.object.id) {
                    (Some(n), _) => format!("{} {n}", e.object.kind),
                    (None, Some(id)) => format!("{} {id}", e.object.kind),
                    (None, None) => e.object.kind.clone(),
                },
                format!("{:?}", e.result).to_lowercase(),
                output::opt(e.detail.as_deref()),
            ]
        })
        .collect();
    output::table(
        &["ID", "AT", "ACTOR", "ACTION", "OBJECT", "RESULT", "DETAIL"],
        &rows,
    );
    Ok(())
}

async fn jobs(client: &Client, cmd: JobsCmd, json: bool) -> Result<(), Error> {
    let row = |j: &Job| {
        vec![
            j.id.to_string(),
            format!("{:?}", j.state).to_lowercase(),
            j.kind.clone(),
            j.target.as_ref().map_or_else(
                || "-".to_owned(),
                |t| format!("{} {}", t.kind, output::opt(t.name.as_deref())),
            ),
            j.progress
                .map_or_else(|| "-".to_owned(), |p| format!("{:.0}%", p * 100.0)),
            output::ts(Some(j.created_at)),
            output::opt(j.message.as_deref()),
        ]
    };
    let headers = [
        "ID", "STATE", "KIND", "TARGET", "PROGRESS", "CREATED", "MESSAGE",
    ];
    match cmd {
        JobsCmd::List { state, paging } => {
            let mut query = Vec::new();
            if let Some(s) = state {
                query.push(("state", s));
            }
            let page: Page<Job> = pages(client, "/jobs", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(row).collect();
                output::table(&headers, &rows);
            }
        }
        JobsCmd::Get { id } => {
            let j: Job = client
                .json("GET", &format!("/jobs/{id}"), &[], None)
                .await?;
            if json {
                output::json(&serde_json::to_value(&j)?);
            } else {
                output::table(&headers, &[row(&j)]);
            }
        }
    }
    Ok(())
}

pub(crate) fn done(json: bool, what: &str, id: Id) {
    if json {
        output::json(&json!({ "id": id, "result": what }));
    } else {
        println!("{what}: {id}");
    }
}
