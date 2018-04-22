extern crate ammonia;
extern crate env_logger;
extern crate html5ever;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate robotparser;
extern crate url;

use reqwest::Client;
use robotparser::RobotFileParser;
use url::Url;

mod html;
mod url_utils;

fn crawl_page(
    url: &str,
    headers: &reqwest::header::Headers,
    text: String,
    cache: Vec<String>,
) -> Option<(bool, Vec<String>, String, Vec<(String, String)>)> {
    let _content_type = headers.get::<reqwest::header::ContentType>();

    if _content_type == None {
        warn!("no Content-Type for {}", url);
        return None;
    }
    let content_type = _content_type.unwrap().subtype();

    if content_type == reqwest::mime::HTML {
        return Some(
            html::find_urls_in_html(Url::parse(url).unwrap(), text, cache).unwrap_or((
                false,
                Vec::new(),
                "".to_string(),
                Vec::new(),
            )),
        );
    }

    return None;
}

fn find_in_robot_cache<'a>(
    object: &str,
    array: Vec<(String, RobotFileParser<'a>)>,
) -> Option<(String, RobotFileParser<'a>)> {
    trace!("finding {} in robot_cache", object);
    for entry in array {
        trace!("discovered {} in robot_cache", entry.0);
        if String::from(object) == entry.0 {
            debug!("found {} in robot cache!", object);
            return Some(entry);
        }
    }

    debug!("couldn't find {} in robot_cache :(", object);
    return None;
}

fn main() {
    // subtract 1 to account for argv[0]
    assert_eq!(std::env::args().count() - 1, 1, "not enough arguments");

    env_logger::init();
    info!("crawler init!");

    main_loop();
}

fn main_loop() {
    _main_loop(std::env::args().nth(1).unwrap().to_string());
}

fn _main_loop(starton: String) {
    let client = Client::new();
    let mut future_urls: Vec<String>;
    let mut future_url_buffer: Vec<String> = Vec::new();
    let mut robots_cache: Vec<(String, RobotFileParser)> = Vec::new();
    let mut fetched_cache: Vec<String> = Vec::new();

    future_url_buffer.push(starton);

    loop {
        future_urls = future_url_buffer.clone();
        future_url_buffer = Vec::new();

        if future_urls.len() == 0 {
            panic!("no more urls???");
        }

        for url in future_urls {
            debug!("url = {}", url);
            let parsed_url = Url::parse(&url).unwrap();
            let mut hostname = String::from(parsed_url.host_str().unwrap()); // TODO Merge with previous line

            if !url_utils::check_if_is_in_url_list(&url, &fetched_cache) {
                info!("[skipping {} (already fetched)]", url);
                continue;
            } else {
                fetched_cache.push(url.clone());
            }

            let mut original_hostname = hostname.clone();

            let mut _robotsok = find_in_robot_cache(&hostname, robots_cache.clone());
            let mut robotsok: (String, RobotFileParser);

            let mut robotstxt_path = parsed_url.clone();
            robotstxt_path.set_path("/robots.txt");

            if _robotsok == None {
                if robots_cache.len() > 512 {
                    debug!("clearing robots_cache");
                    robots_cache.clear();
                }

                debug!("fetching robots.txt, aka {}", robotstxt_path);
                let robotstxt = RobotFileParser::new(&robotstxt_path);
                robotstxt.read();
                robotsok = (String::from(original_hostname), robotstxt);
                robots_cache.push(robotsok.clone());
                debug!("finished, in cache");
            } else {
                robotsok = _robotsok.unwrap();
            }

            if robotsok.1.can_fetch("twentiethbot", &url) {
                info!("fetching {}!", url);
                let response = client.get(&url).send();

                if response.is_err() {
                    warn!("request to {} failed: {:?}", url, response);
                } else {
                    let mut response = response.unwrap();
                    let text = response.text().unwrap_or("???".to_string());
                    let mut _found_urls = crawl_page(
                        &url,
                        &response.headers(),
                        text.clone(),
                        fetched_cache.clone(),
                    );

                    if _found_urls != None {
                        future_url_buffer.append(&mut _found_urls.clone().unwrap().1);
                    }

                    let found_urls =
                        _found_urls.unwrap_or((true, Vec::new(), "".to_string(), Vec::new()));

                    if found_urls.0 && response.status() == reqwest::StatusCode::Ok {
                        match found_urls.2.as_str() {
                            "html" => {
                                let meta = found_urls
                                    .3
                                    .iter()
                                    .map(|x| format!("{}={}", x.0, x.1))
                                    .collect::<Vec<String>>()
                                    .join(";");
                                println!(
                                    "{}\t{}\t{}",
                                    url,
                                    ammonia::Builder::default()
                                        .clean_content_tags(
                                            vec!["head", "style", "script"].into_iter().collect()
                                        )
                                        .tags(std::collections::HashSet::new())
                                        .clean(&text)
                                        .to_string()
                                        .replace("\t", " ")
                                        .replace("\n", " "),
                                    meta
                                );
                            }
                            _ => {
                                println!("{}\t{}\t", url, text);
                            }
                        }
                    }
                }
            } else {
                warn!("ignoring {} (forbidden by robots.txt)", url);
            }
        }
    }
}
