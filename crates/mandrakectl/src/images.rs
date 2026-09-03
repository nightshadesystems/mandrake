//! `images` commands: one API call each.

use mandrake_core::{
    api::{Job, Page},
    image::{CatalogueEntry, Image, ImageSource},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    cli::{ImagesCmd, SourcesCmd},
    client::{Client, Error},
    cmd::{done, pages},
    output,
    storage::metadata_value,
};

/// `{ "items": [...] }`.
#[derive(Debug, Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

/// A job as key/value lines, or JSON.
pub(crate) fn print_job(j: &Job, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(j)?);
    } else {
        output::kv(&[
            ("job", j.id.to_string()),
            ("state", format!("{:?}", j.state).to_lowercase()),
            ("kind", j.kind.clone()),
            (
                "target",
                j.target.as_ref().map_or_else(
                    || "-".to_owned(),
                    |t| {
                        format!(
                            "{} {}",
                            t.kind,
                            t.id.map_or_else(|| "-".to_owned(), |i| i.to_string())
                        )
                    },
                ),
            ),
            ("message", output::opt(j.message.as_deref())),
        ]);
    }
    Ok(())
}

pub async fn run(client: &Client, cmd: ImagesCmd, json: bool) -> Result<(), Error> {
    match cmd {
        ImagesCmd::List {
            r#type,
            state,
            paging,
        } => {
            let mut query = Vec::new();
            if let Some(t) = r#type {
                query.push(("type", t));
            }
            if let Some(s) = state {
                query.push(("state", s));
            }
            let page: Page<Image> = pages(client, "/images", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(image_row).collect();
                output::table(&IMAGE_HEADERS, &rows);
                if let Some(c) = page.next_cursor {
                    eprintln!("more: --cursor {c}");
                }
            }
        }
        ImagesCmd::Get { id } => {
            let i: Image = client
                .json("GET", &format!("/images/{id}"), &[], None)
                .await?;
            print_image(&i, json)?;
        }
        ImagesCmd::Available { source, r#type } => available(client, source, r#type, json).await?,
        ImagesCmd::Import {
            name,
            version,
            source,
            url,
            sha256,
            r#type,
            pool,
            metadata,
        } => {
            if source.is_none() && url.is_none() {
                return Err(Error::Config(
                    "give --source, or --url with --sha256 and --type".to_owned(),
                ));
            }
            let body = json!({
                "name": name,
                "version": version,
                "source_id": source,
                "url": url,
                "sha256": sha256,
                "type": r#type,
                "pool": pool,
                "metadata": metadata_value(&metadata),
            });
            let j: Job = client
                .json("POST", "/images/import", &[], Some(&body))
                .await?;
            print_job(&j, json)?;
        }
        ImagesCmd::Update { id, metadata } => {
            let body = metadata_value(&metadata)
                .ok_or_else(|| Error::Config("nothing to update".to_owned()))?;
            let i: Image = client
                .json("PATCH", &format!("/images/{id}"), &[], Some(&body))
                .await?;
            print_image(&i, json)?;
        }
        ImagesCmd::Delete { id } => {
            client
                .empty("DELETE", &format!("/images/{id}"), None)
                .await?;
            done(json, "deleted", id);
        }
        ImagesCmd::Sources(cmd) => sources(client, cmd, json).await?,
    }
    Ok(())
}

async fn available(
    client: &Client,
    source: Option<mandrake_core::Id>,
    r#type: Option<String>,
    json: bool,
) -> Result<(), Error> {
    let mut query = Vec::new();
    if let Some(s) = source {
        query.push(("source_id", s.to_string()));
    }
    if let Some(t) = r#type {
        query.push(("type", t));
    }
    let list: Items<CatalogueEntry> = client
        .json("GET", "/images/available", &query, None)
        .await?;
    if json {
        output::json(&json!({ "items": list.items }));
        return Ok(());
    }
    let rows: Vec<Vec<String>> = list
        .items
        .iter()
        .map(|e| {
            vec![
                e.source_name.clone(),
                format!("{}@{}", e.name, e.version),
                e.type_.to_string(),
                output::opt(e.os.as_deref()),
                output::size(e.size_bytes),
                if e.imported {
                    e.image_id
                        .map_or_else(|| "yes".to_owned(), |i| i.to_string())
                } else {
                    "-".to_owned()
                },
            ]
        })
        .collect();
    output::table(
        &["SOURCE", "IMAGE", "TYPE", "OS", "SIZE", "IMPORTED"],
        &rows,
    );
    Ok(())
}

const IMAGE_HEADERS: [&str; 7] = ["ID", "IMAGE", "TYPE", "STATE", "OS", "SIZE", "LOCATION"];

fn image_row(i: &Image) -> Vec<String> {
    vec![
        i.id.to_string(),
        format!("{}@{}", i.name, i.version),
        i.type_.to_string(),
        match (i.state, i.progress) {
            (s, Some(p))
                if s != mandrake_core::image::ImageState::Ready
                    && s != mandrake_core::image::ImageState::Failed =>
            {
                format!("{s} {:.0}%", p * 100.0)
            }
            (s, _) => s.to_string(),
        },
        output::opt(i.os.as_deref()),
        output::size(i.size_bytes),
        i.dataset
            .clone()
            .or_else(|| i.path.clone())
            .unwrap_or_else(|| "-".to_owned()),
    ]
}

fn print_image(i: &Image, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(i)?);
        return Ok(());
    }
    let mut pairs = vec![
        ("id", i.id.to_string()),
        ("name", i.name.clone()),
        ("version", i.version.clone()),
        ("type", i.type_.to_string()),
        ("state", i.state.to_string()),
        ("sha256", i.sha256.clone()),
        ("size", output::size(i.size_bytes)),
        ("pool", output::opt(i.pool.as_deref())),
        ("dataset", output::opt(i.dataset.as_deref())),
        ("path", output::opt(i.path.as_deref())),
        ("source", output::opt(i.source_name.as_deref())),
        ("url", output::opt(i.url.as_deref())),
        ("os", output::opt(i.os.as_deref())),
        (
            "in use by",
            i.in_use_by
                .map_or_else(|| "-".to_owned(), |n| n.to_string()),
        ),
        ("created", i.created_at.to_rfc3339()),
        ("imported", output::ts(i.imported_at)),
    ];
    if let Some(e) = &i.error {
        pairs.push(("error", e.clone()));
    }
    if let Some(m) = &i.metadata {
        if let Some(n) = &m.display_name {
            pairs.push(("display name", n.clone()));
        }
        if let Some(n) = &m.description {
            pairs.push(("description", n.clone()));
        }
    }
    output::kv(&pairs);
    Ok(())
}

const SOURCE_HEADERS: [&str; 7] = ["ID", "NAME", "URL", "STATE", "IMAGES", "REFRESHED", "ERROR"];

fn source_row(s: &ImageSource) -> Vec<String> {
    vec![
        s.id.to_string(),
        if s.builtin {
            format!("{} (built-in)", s.name)
        } else {
            s.name.clone()
        },
        s.url.clone(),
        if !s.enabled {
            "disabled".to_owned()
        } else if s.verified {
            "verified".to_owned()
        } else if s.public_key.is_some() {
            "not verified".to_owned()
        } else {
            "no key".to_owned()
        },
        s.image_count.to_string(),
        output::ts(s.last_refreshed_at),
        output::opt(s.last_error.as_deref()),
    ]
}

fn print_source(s: &ImageSource, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(s)?);
    } else {
        output::table(&SOURCE_HEADERS, &[source_row(s)]);
    }
    Ok(())
}

async fn sources(client: &Client, cmd: SourcesCmd, json: bool) -> Result<(), Error> {
    match cmd {
        SourcesCmd::List => {
            let list: Items<ImageSource> = client.json("GET", "/images/sources", &[], None).await?;
            if json {
                output::json(&json!({ "items": list.items }));
            } else {
                let rows: Vec<_> = list.items.iter().map(source_row).collect();
                output::table(&SOURCE_HEADERS, &rows);
            }
        }
        SourcesCmd::Get { id } => {
            let s: ImageSource = client
                .json("GET", &format!("/images/sources/{id}"), &[], None)
                .await?;
            print_source(&s, json)?;
        }
        SourcesCmd::Add {
            name,
            url,
            public_key,
            disabled,
        } => {
            let body = json!({
                "name": name,
                "url": url,
                "public_key": public_key,
                "enabled": !disabled,
            });
            let s: ImageSource = client
                .json("POST", "/images/sources", &[], Some(&body))
                .await?;
            print_source(&s, json)?;
        }
        SourcesCmd::Update {
            id,
            name,
            url,
            public_key,
            no_key,
            enable,
            disable,
        } => {
            let mut body = Map::new();
            if let Some(n) = name {
                body.insert("name".to_owned(), Value::String(n));
            }
            if let Some(u) = url {
                body.insert("url".to_owned(), Value::String(u));
            }
            if let Some(k) = public_key {
                body.insert("public_key".to_owned(), Value::String(k));
            } else if no_key {
                body.insert("public_key".to_owned(), Value::Null);
            }
            if enable || disable {
                body.insert("enabled".to_owned(), Value::Bool(enable));
            }
            if body.is_empty() {
                return Err(Error::Config("nothing to update".to_owned()));
            }
            let s: ImageSource = client
                .json(
                    "PATCH",
                    &format!("/images/sources/{id}"),
                    &[],
                    Some(&Value::Object(body)),
                )
                .await?;
            print_source(&s, json)?;
        }
        SourcesCmd::Remove { id } => {
            client
                .empty("DELETE", &format!("/images/sources/{id}"), None)
                .await?;
            done(json, "removed", id);
        }
        SourcesCmd::Refresh { id } => {
            let s: ImageSource = client
                .json("POST", &format!("/images/sources/{id}/refresh"), &[], None)
                .await?;
            print_source(&s, json)?;
        }
    }
    Ok(())
}
