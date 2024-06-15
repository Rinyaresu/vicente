use actix_cors::Cors;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use chrono::{DateTime, Duration, Utc};
use reqwest;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
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

#[get("/opml")]
async fn get_opml() -> impl Responder {
    match File::open("public/rss.opml") {
        Ok(file) => {
            let parser = EventReader::new(BufReader::new(file));
            let mut feeds = Vec::new();

            for e in parser {
                match e {
                    Ok(XmlEvent::StartElement {
                        name, attributes, ..
                    }) => {
                        if name.local_name == "outline" {
                            let mut feed = Feed {
                                title: String::new(),
                                xml_url: String::new(),
                                html_url: String::new(),
                            };
                            for attr in attributes {
                                match attr.name.local_name.as_str() {
                                    "title" => feed.title = attr.value.clone(),
                                    "xmlUrl" => feed.xml_url = attr.value.clone(),
                                    "htmlUrl" => feed.html_url = attr.value.clone(),
                                    _ => (),
                                }
                            }
                            if !feed.xml_url.is_empty() {
                                feeds.push(feed);
                            }
                        }
                    }
                    Err(_) => break,
                    _ => {}
                }
            }

            HttpResponse::Ok().json(feeds)
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to open OPML file"),
    }
}

#[get("/articles")]
async fn get_articles(query: web::Query<HashMap<String, String>>) -> impl Responder {
    let date_filter = Utc::now() - Duration::days(5);

    match get_feeds().await {
        Ok(feeds) => {
            let mut articles = Vec::new();
            let mut content_encoded_count = 0;
            let mut empty_content_encoded_count = 0;

            for feed in feeds {
                println!("Fetching articles from feed: {:?}", feed.xml_url);
                if let Ok(response) = reqwest::get(&feed.xml_url).await {
                    if let Ok(body) = response.text().await {
                        let parser = EventReader::new(body.as_bytes());
                        let mut current_article = Article {
                            title: String::new(),
                            link: String::new(),
                            description: String::new(),
                            pub_date: String::new(),
                            feed_title: feed.title.clone(),
                            content_encoded: String::new(),
                        };
                        let mut inside_item = false;
                        let mut inside_content_encoded = false;
                        let mut current_element = String::new();
                        let mut text = String::new();

                        for e in parser {
                            match e {
                                Ok(XmlEvent::StartElement { name, .. }) => {
                                    if name.local_name == "item" {
                                        inside_item = true;
                                    } else if inside_item {
                                        current_element = name.local_name.clone();
                                        text = String::new();
                                        if name.local_name == "encoded"
                                            && name.namespace.as_deref()
                                                == Some("http://purl.org/rss/1.0/modules/content/")
                                        {
                                            inside_content_encoded = true;
                                        }
                                    }
                                }
                                Ok(XmlEvent::Characters(content)) => {
                                    if inside_item {
                                        if inside_content_encoded {
                                            current_article.content_encoded.push_str(&content);
                                        } else {
                                            text.push_str(&content);
                                        }
                                    }
                                }
                                Ok(XmlEvent::CData(content)) => {
                                    if inside_item {
                                        if inside_content_encoded {
                                            current_article.content_encoded.push_str(&content);
                                        } else {
                                            text.push_str(&content);
                                        }
                                    }
                                }
                                Ok(XmlEvent::EndElement { name }) => {
                                    if inside_item {
                                        match name.local_name.as_str() {
                                            "title" => {
                                                current_article.title = text.clone();
                                                println!("Found article title: {}", text);
                                            }
                                            "link" => {
                                                current_article.link = text.clone();
                                                println!("Found article link: {}", text);
                                            }
                                            "description" => {
                                                current_article.description = text.clone();
                                                println!("Found article description: {}", text);
                                            }
                                            "pubDate" => {
                                                current_article.pub_date = text.clone();
                                                println!("Found article pubDate: {}", text);
                                            }
                                            "encoded" => {
                                                if name.namespace.as_deref()
                                                    == Some(
                                                        "http://purl.org/rss/1.0/modules/content/",
                                                    )
                                                {
                                                    content_encoded_count += 1;
                                                    inside_content_encoded = false;
                                                    println!(
                                                        "Found article content_encoded: {}",
                                                        current_article.content_encoded
                                                    );
                                                }
                                            }
                                            "item" => {
                                                let pub_date = DateTime::parse_from_rfc2822(
                                                    &current_article.pub_date,
                                                )
                                                .map(|dt| dt.with_timezone(&Utc))
                                                .unwrap_or_else(|_| Utc::now());
                                                if pub_date < date_filter {
                                                    continue;
                                                }

                                                if current_article.content_encoded.is_empty() {
                                                    empty_content_encoded_count += 1;
                                                }
                                                articles.push(current_article.clone());
                                                println!("Added article: {:?}", current_article);
                                                current_article = Article {
                                                    title: String::new(),
                                                    link: String::new(),
                                                    description: String::new(),
                                                    pub_date: String::new(),
                                                    feed_title: feed.title.clone(),
                                                    content_encoded: String::new(),
                                                };
                                                inside_item = false;
                                            }
                                            _ => (),
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("Error parsing XML: {:?}", e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            println!("Total content:encoded found: {}", content_encoded_count);
            println!(
                "Total content:encoded empty: {}",
                empty_content_encoded_count
            );
            HttpResponse::Ok().json(articles)
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to get feeds"),
    }
}

async fn get_feeds() -> Result<Vec<Feed>, std::io::Error> {
    let file = File::open("public/rss.opml")?;
    let parser = EventReader::new(BufReader::new(file));
    let mut feeds = Vec::new();

    for e in parser {
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                if name.local_name == "outline" {
                    let mut feed = Feed {
                        title: String::new(),
                        xml_url: String::new(),
                        html_url: String::new(),
                    };
                    for attr in attributes {
                        match attr.name.local_name.as_str() {
                            "title" => feed.title = attr.value.clone(),
                            "xmlUrl" => feed.xml_url = attr.value.clone(),
                            "htmlUrl" => feed.html_url = attr.value.clone(),
                            _ => (),
                        }
                    }
                    if !feed.xml_url.is_empty() {
                        feeds.push(feed);
                    }
                }
            }
            Err(e) => {
                println!("Error parsing OPML: {:?}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(feeds)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
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
