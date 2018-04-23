extern crate ammonia;
extern crate env_logger;
extern crate html5ever;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate robotparser;
extern crate url;

// see issue #7
//#[cfg(test)]
//extern crate iron;

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
    _main_loop(std::env::args().nth(1).unwrap().to_string(), true);
}

fn _main_loop(starton: String, panic: bool) -> Vec<String> {
    // see issue #7
    let client = Client::new();
    let mut future_urls: Vec<String>;
    let mut future_url_buffer: Vec<String> = Vec::new();
    let mut robots_cache: Vec<(String, RobotFileParser)> = Vec::new();
    let mut fetched_cache: Vec<String> = Vec::new();

    #[allow(unused_mut)]
    let mut all_found_urls: Vec<String> = Vec::new();

    future_url_buffer.push(starton);

    loop {
        future_urls = future_url_buffer.clone();
        future_url_buffer = Vec::new();

        if future_urls.len() == 0 {
            if panic {
                panic!("no more urls???");
            } else {
                return all_found_urls;
            }
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

                        // Don't append unless we're testing to save memory
                        #[cfg(test)]
                        {
                            all_found_urls.append(&mut found_urls.1.clone());
                        }
                    }
                }
            } else {
                warn!("ignoring {} (forbidden by robots.txt)", url);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // see issue #7
    //use iron::{Iron, IronResult, Headers};
    //use iron::response::Response;
    //use iron::request::Request;
    //use iron::status;
    //use iron::middleware::Chain;
    //use iron::headers::ContentType;
    //use iron::mime::{Mime, TopLevel, SubLevel};
    //use iron::typemap::TypeMap;
    use std;

    use ::*;

    // see issue #7
    //#[test]
    //fn __main_loop() {
    //    fn handler(req: &mut Request) -> IronResult<Response> {
    //        let mut mime = Headers::new();
    //        mime.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, Vec::new())));

    //        Ok(Response {
    //            headers: mime,
    //            status: Some(status::Ok),
    //            body: Some(Box::new(match req.url.path().join("/").as_str() {
    //                "" => "<a href='file'></a><a href='file1'></a>",
    //                "file" => "<a href='/file1'></a>",
    //                "file1" => "<a href='/file'></a>",
    //                _ => "not found"
    //            })),
    //            extensions: TypeMap::new()
    //        })
    //    }

    //    let child = std::thread::spawn(|| Iron::new(Chain::new(handler)).http("localhost:9999").unwrap());

    //    let f: Vec<String> = Vec::new();
    //    assert_eq!(_main_loop("http://localhost:9999/".to_string(), false), f);
    //}

    #[test]
    fn _find_in_robot_cache() {
        let mut fake_cache = Vec::new();
        assert_eq!(
            find_in_robot_cache("https://google.com", fake_cache.clone()),
            None
        );
        fake_cache.push((
            "https://google.com".to_string(),
            RobotFileParser::new("https://google.com/robots.txt"),
        ));
        assert_eq!(
            find_in_robot_cache("https://google.com", fake_cache),
            Some((
                "https://google.com".to_string(),
                RobotFileParser::new("https://google.com/robots.txt")
            ))
        );
    }

    #[test]
    fn _crawl_page() {
        #[allow(non_snake_case)]
        fn S(inp: &str) -> String {
            return inp.to_string();
        }

        let mut headers = reqwest::header::Headers::new();
        headers.set(reqwest::header::ContentType::html());
        assert_eq!(
            crawl_page(
                "https://google.com",
                &headers,
                S("<a href='news'></a><a href='gmail'></a>"),
                Vec::new()
            ),
            html::find_urls_in_html(
                Url::parse("https://google.com/").unwrap(),
                S("<a href='news'></a><a href='gmail'></a>"),
                Vec::new()
            )
        );
        assert_eq!(
            crawl_page(
                "https://google.com",
                &reqwest::header::Headers::new(),
                S("dummy text"),
                Vec::new()
            ),
            None
        );
    }
}
