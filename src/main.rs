use actix_cors::Cors;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use xml::reader::{EventReader, XmlEvent};

#[derive(Serialize, Clone, Debug)]
struct Feed {
    title: String,
    xml_url: String,
    html_url: String,
}

#[derive(Serialize, Clone, Debug)]
struct Article {
    title: String,
    link: String,
    description: String,
    pub_date: String,
    feed_title: String,
    content_encoded: String,
}

type Cache = Arc<Mutex<HashMap<String, Vec<Article>>>>;

#[get("/opml")]
async fn get_opml() -> impl Responder {
    match File::open("public/rss.opml") {
        Ok(file) => {
            let parser = EventReader::new(BufReader::new(file));
            let feeds: Vec<Feed> = parser
                .into_iter()
                .filter_map(|e| match e {
                    Ok(XmlEvent::StartElement {
                        name, attributes, ..
                    }) if name.local_name == "outline" => {
                        let mut feed = Feed {
                            title: String::new(),
                            xml_url: String::new(),
                            html_url: String::new(),
                        };
                        for attr in attributes {
                            match attr.name.local_name.as_str() {
                                "title" => feed.title = attr.value,
                                "xmlUrl" => feed.xml_url = attr.value,
                                "htmlUrl" => feed.html_url = attr.value,
                                _ => (),
                            }
                        }
                        if !feed.xml_url.is_empty() {
                            Some(feed)
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .collect();

            HttpResponse::Ok().json(feeds)
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to open OPML file"),
    }
}

#[get("/articles")]
async fn get_articles(
    _query: web::Query<HashMap<String, String>>,
    data: web::Data<AppState>,
) -> impl Responder {
    println!("Starting to fetch articles...");

    let date_filter = Utc::now() - Duration::days(5);
    let cache = data.cache.clone();

    match get_feeds().await {
        Ok(feeds) => {
            let client = Client::new();
            let semaphore = Arc::new(Semaphore::new(10)); // Limite de 10 requests concorrentes

            let article_futures = feeds.into_iter().map(|feed| {
                let client = client.clone();
                let semaphore = semaphore.clone();
                let cache = cache.clone();
                let xml_url = feed.xml_url.clone();
                let feed_title = feed.title.clone();

                tokio::spawn(async move {
                    {
                        let cache_lock = cache.lock().await;
                        if let Some(cached_articles) = cache_lock.get(&feed_title) {
                            println!("Cache hit for feed: {}", feed_title);
                            return cached_articles.clone();
                        }
                    }

                    println!("Fetching feed: {}", feed_title);
                    let permit = semaphore.acquire_owned().await.unwrap(); // Adquirir o permit
                    let articles = fetch_articles_from_feed(
                        client,
                        xml_url,
                        feed_title.clone(),
                        date_filter,
                        permit,
                    )
                    .await;

                    {
                        let mut cache_lock = cache.lock().await;
                        cache_lock.insert(feed_title.clone(), articles.clone());
                    }
                    articles
                })
            });

            let articles = join_all(article_futures)
                .await
                .into_iter()
                .filter_map(|res| res.ok())
                .flatten()
                .collect::<Vec<_>>();

            println!(
                "Finished fetching articles. Total articles fetched: {}",
                articles.len()
            );
            HttpResponse::Ok().json(articles)
        }
        Err(_) => {
            println!("Failed to get feeds");
            HttpResponse::InternalServerError().body("Failed to get feeds")
        }
    }
}

async fn fetch_articles_from_feed(
    client: Client,
    xml_url: String,
    feed_title: String,
    date_filter: chrono::DateTime<Utc>,
    _permit: OwnedSemaphorePermit,
) -> Vec<Article> {
    let mut articles = Vec::new();

    println!("Sending request to: {}", xml_url);
    if let Ok(response) = client.get(&xml_url).send().await {
        if let Ok(body) = response.text().await {
            println!("Parsing articles from feed: {}", feed_title);
            let parser = EventReader::new(body.as_bytes());
            let mut current_article = Article {
                title: String::new(),
                link: String::new(),
                description: String::new(),
                pub_date: String::new(),
                feed_title: feed_title.clone(),
                content_encoded: String::new(),
            };
            let mut inside_item = false;
            let mut inside_content_encoded = false;
            let mut text = String::new();

            for e in parser {
                match e {
                    Ok(XmlEvent::StartElement { name, .. }) => {
                        if name.local_name == "item" {
                            inside_item = true;
                        } else if inside_item {
                            text.clear();
                            if name.local_name == "encoded"
                                && name.namespace.as_deref()
                                    == Some("http://purl.org/rss/1.0/modules/content/")
                            {
                                inside_content_encoded = true;
                            }
                        }
                    }
                    Ok(XmlEvent::Characters(content)) if inside_item => {
                        if inside_content_encoded {
                            current_article.content_encoded.push_str(&content);
                        } else {
                            text.push_str(&content);
                        }
                    }
                    Ok(XmlEvent::CData(content)) if inside_item => {
                        if inside_content_encoded {
                            current_article.content_encoded.push_str(&content);
                        } else {
                            text.push_str(&content);
                        }
                    }
                    Ok(XmlEvent::EndElement { name }) if inside_item => {
                        match name.local_name.as_str() {
                            "title" => current_article.title = text.clone(),
                            "link" => current_article.link = text.clone(),
                            "description" => current_article.description = text.clone(),
                            "pubDate" => current_article.pub_date = text.clone(),
                            "encoded"
                                if name.namespace.as_deref()
                                    == Some("http://purl.org/rss/1.0/modules/content/") =>
                            {
                                inside_content_encoded = false
                            }
                            "item" => {
                                let pub_date =
                                    DateTime::parse_from_rfc2822(&current_article.pub_date)
                                        .map(|dt| dt.with_timezone(&Utc))
                                        .unwrap_or_else(|_| Utc::now());
                                if pub_date >= date_filter
                                    && !current_article.content_encoded.is_empty()
                                {
                                    articles.push(current_article.clone());
                                    println!("Added article: {}", current_article.title);
                                }
                                current_article = Article {
                                    title: String::new(),
                                    link: String::new(),
                                    description: String::new(),
                                    pub_date: String::new(),
                                    feed_title: feed_title.clone(),
                                    content_encoded: String::new(),
                                };
                                inside_item = false;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            println!("Finished parsing articles from feed: {}", feed_title);
        }
    }

    articles
}

async fn get_feeds() -> Result<Vec<Feed>, std::io::Error> {
    println!("Reading feeds from OPML file...");
    let file = File::open("public/rss.opml")?;
    let parser = EventReader::new(BufReader::new(file));
    let feeds: Vec<Feed> = parser
        .into_iter()
        .filter_map(|e| match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "outline" => {
                let mut feed = Feed {
                    title: String::new(),
                    xml_url: String::new(),
                    html_url: String::new(),
                };
                for attr in attributes {
                    match attr.name.local_name.as_str() {
                        "title" => feed.title = attr.value,
                        "xmlUrl" => feed.xml_url = attr.value,
                        "htmlUrl" => feed.html_url = attr.value,
                        _ => (),
                    }
                }
                if !feed.xml_url.is_empty() {
                    Some(feed)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();

    println!("Total feeds found: {}", feeds.len());
    Ok(feeds)
}

struct AppState {
    cache: Cache,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting server...");
    let cache: Cache = Arc::new(Mutex::new(HashMap::new()));

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                cache: cache.clone(),
            }))
            .wrap(
                Cors::default()
                    .allowed_origin("http://192.168.1.91:4321")
                    .allowed_methods(vec!["GET", "POST"])
                    .allowed_headers(vec![
                        actix_web::http::header::CONTENT_TYPE,
                        actix_web::http::header::ACCEPT,
                    ])
                    .max_age(3600),
            )
            .service(get_opml)
            .service(get_articles)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
