extern crate env_logger;
extern crate htmlstream;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate regex;
extern crate reqwest;
extern crate robotparser;
extern crate url;

use reqwest::Client;
use robotparser::RobotFileParser;
use url::Url;

fn get_attribute_for_elem<'a>(elem: &str) -> Option<&'a str> {
    match elem {
        "a" => Some("href"),
        "script" => Some("src"),
        "link" => Some("href"),
        "img" => Some("src"),
        _ => None,
    }
}

fn get_root_domain(url: &str) -> Option<String> {
    debug!("getting root domain for {}", url);
    let _parsed_url = Url::parse(url);

    if _parsed_url.is_err() {
        warn!("failed to parse URL in get_root_domain ({})", url);
        return None;
    }

    let parsed_url = _parsed_url.unwrap();
    let hostname = parsed_url.host_str();
    if hostname == None {
        warn!(
            "failed to find hostname for URL in get_root_domain ({})",
            url
        );
        return None;
    }

    let subdomainless_hostname = hostname.unwrap().splitn(2, '.').nth(1);

    if subdomainless_hostname == None {
        warn!(
            "invalid URL (likely missing TLD) passed to get_root_domain ({})",
            url
        );
        return None;
    } else if hostname.unwrap().contains('.') {
        let mut returned_url = parsed_url.clone();
        let set_host_result = returned_url.set_host(Some(hostname.unwrap()));
        if set_host_result.is_err() {
            warn!(
                "error setting host of {} to {}: {:?}",
                returned_url.as_str(),
                hostname.unwrap(),
                set_host_result
            );
            return None;
        }
        returned_url.set_path("/");

        debug!("found {} to be main domain!", returned_url.as_str());
        return Some(returned_url.as_str().to_string());
    } else {
        debug!("{} is main domain, ignore", hostname.unwrap());
        return None;
    }
}

fn add_urls_to_vec(urls: Option<Vec<String>>, into: &mut Vec<String>, cache: &Vec<String>) {
    if urls != None {
        for url in urls.unwrap() {
            if check_if_is_in_url_list(&url, &into) && check_if_is_in_url_list(&url, &cache) {
                trace!("found url {}", url);
                into.push(url);
            } else {
                trace!("found duplicate url {}", url);
            }
        }
    }
}

fn find_urls_in_html(
    original_url: &Url,
    html: htmlstream::HTMLTagIterator,
    fetched_cache: &Vec<String>,
) -> Vec<String> {
    let mut returned_vec = Vec::new();

    for (_pos, tag) in html {
        if tag.state == htmlstream::HTMLTagState::Opening && tag.attributes != "" {
            let _attribute_name = get_attribute_for_elem(&tag.name);

            if _attribute_name == None {
                continue;
            }
            let attribute_name = _attribute_name.unwrap();

            for attribute_set in tag.attributes.split(" ") {
                if attribute_set.contains("=") {
                    let mut attribute_splitted = attribute_set.split("=\"");

                    if attribute_splitted.nth(0).unwrap() == attribute_name {
                        let mut attribute_splitted_collection: Vec<&str> =
                            attribute_splitted.collect();
                        add_urls_to_vec(
                            repair_suggested_url(original_url, attribute_splitted_collection),
                            &mut returned_vec,
                            fetched_cache,
                        );
                    }
                }
            }
        }
    }

    return returned_vec;
}

fn repair_suggested_url(original_url: &Url, attribute_splitted: Vec<&str>) -> Option<Vec<String>> {
    let mut returned_vec: Vec<String> = Vec::new();
    let mut found_url = attribute_splitted[0]
        .replace("\n", "")
        .split("#")
        .nth(0)
        .unwrap()
        .to_string();

    // Some urls contain an =, particularly in GET parameters.
    // This accounts for them.
    let mut current = 0;
    for found_url_part in attribute_splitted {
        current += 1;

        if current > 1 {
            // Ensure current > 1 so we don't get duplicates in the path
            found_url.push_str(found_url_part);
        }
    }

    if found_url.len() == 0 || found_url.len() == 1 {
        return None;
    }

    // Remove the final quote.
    found_url.pop();

    let mut parsed_found_url;

    if found_url.starts_with(".") || found_url.starts_with("?") {
        parsed_found_url = original_url.join(&found_url).unwrap();
    } else if found_url.starts_with("/") {
        if found_url.chars().nth(1).unwrap_or(' ') != '/' {
            parsed_found_url = original_url.clone();
            parsed_found_url.set_path("/");
        } else if found_url.starts_with("//") {
            let mut modified_url = "".to_string();
            modified_url.push_str("https:");
            modified_url.push_str(&found_url);
            parsed_found_url = Url::parse(&modified_url).unwrap();
        } else {
            warn!("strange url found: {}", found_url);
            return None;
        }
    } else {
        let _parsed_found_url = Url::parse(&found_url);

        if _parsed_found_url.is_err() {
            warn!("strange url found: {}", found_url);
            return None;
        }
        parsed_found_url = _parsed_found_url.unwrap();
    }

    returned_vec.push(parsed_found_url.as_str().to_string());

    let main_domain = get_root_domain(parsed_found_url.as_str());
    if main_domain != None {
        returned_vec.push(main_domain.unwrap());
    }

    return Some(returned_vec);
}

fn crawl_page(
    url: &str,
    headers: &reqwest::header::Headers,
    _text: Result<String, reqwest::Error>,
    cache: &Vec<String>,
) -> Option<(Vec<String>, bool)> {
    let _content_type = headers.get::<reqwest::header::ContentType>();

    if _content_type == None {
        warn!("no Content-Type for {}", url);
        return None;
    }
    let content_type = _content_type.unwrap().subtype();

    if _text.is_err() {
        warn!("error getting text for {} ({:?})", url, _text);
        return None;
    }
    let text = _text.unwrap();

    if content_type == reqwest::mime::HTML {
        trace!("Started parsing html...");
        let html = htmlstream::tag_iter(text.as_str());
        trace!("Finished!");
        let mut index_url = true;

        lazy_static! {
            static ref ROBOTS_REGEX: regex::Regex = regex::Regex::new(".*name=.(robots|twentiethbot)..*").unwrap();
        }

        for (_pos, tag) in html {
            if (tag.state == htmlstream::HTMLTagState::Opening
                || tag.state == htmlstream::HTMLTagState::SelfClosing)
                && tag.name == "meta" && ROBOTS_REGEX.is_match(&tag.attributes)
            {
                for attribute_set in tag.attributes.split(" ") {
                    let mut attribute_split = attribute_set.split("=\"");
                    let mut _robotsvalue = attribute_split.nth(1);
                    if attribute_split.clone().count() == 1 || _robotsvalue.unwrap_or("").len() == 0
                        || _robotsvalue == None
                    {
                        debug!(
                            "Odd <meta> tag on {}: {} (attribute_set={:?})",
                            url, tag.html, attribute_set
                        );
                        continue;
                    } else if !attribute_set.starts_with("content") {
                        continue;
                    }
                    let mut robotsvalue = _robotsvalue.unwrap().to_string();

                    if robotsvalue.ends_with("\"") || robotsvalue.ends_with("\"") {
                        robotsvalue.pop();
                    }

                    for robots_command in robotsvalue.split(",").map(|x| x.to_lowercase()) {
                        if robots_command == "nofollow" {
                            return None;
                        } else if robots_command == "noindex" {
                            index_url = false;
                        }
                        // Other values, like noodp and noarchive/nocache, are currently
                        // irrelevant.
                    }
                }
            }
        }

        return Some((
            find_urls_in_html(
                &Url::parse(url).unwrap(),
                htmlstream::tag_iter(text.as_str()),
                cache,
            ),
            index_url,
        ));
    }

    return None;
}

fn check_if_is_in_url_list(object: &str, array: &Vec<String>) -> bool {
    trace!("finding {} in url list", object);
    for entry in array {
        trace!("discovered {} in url list", entry);
        if &String::from(object) == entry {
            debug!("found {} in url list, discard it", object);
            return false;
        }
    }

    debug!("not found, insert {}", object);
    return true;
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
    if std::env::args().count() != 2 {
        println!("crawler: Crawls the Web for URLs using a RegEx.");
        println!("         Usage: crawler <url>");
        return;
    }
    env_logger::init();
    info!("crawler init!");

    let client = Client::new();
    let mut future_urls: Vec<String>;

    let mut future_url_buffer: Vec<String> = vec![std::env::args().nth(1).unwrap().to_string()];
    let mut robots_cache: Vec<(String, RobotFileParser)> = Vec::new();
    let mut fetched_cache: Vec<String> = Vec::new();
    let mut all_found_urls: Vec<String> = Vec::new();

    loop {
        future_urls = future_url_buffer.clone();
        future_url_buffer = Vec::new();

        for url in future_urls {
            debug!("url = {}", url);
            let parsed_url = Url::parse(&url).unwrap();
            let mut hostname = String::from(parsed_url.host_str().unwrap()); // TODO Merge with previous line

            if !check_if_is_in_url_list(&url, &fetched_cache) {
                info!("==> Skipping {} (already fetched)", url);
                continue;
            } else {
                fetched_cache.push(url.clone());
            }

            let mut original_hostname = hostname.clone();

            let mut _robotsok = find_in_robot_cache(&hostname, robots_cache.clone());
            let mut robotsok: (String, RobotFileParser);

            let mut _hostname = String::from(parsed_url.scheme());
            _hostname.push_str("://");
            _hostname.push_str(&hostname);
            _hostname.push_str("/robots.txt");
            hostname = _hostname;

            if _robotsok == None {
                if robots_cache.len() > 512 {
                    debug!("clearing robots_cache");
                    robots_cache.clear();
                }

                debug!("fetching robots.txt, aka {}", hostname);
                let robotstxt = RobotFileParser::new(&hostname);
                robotstxt.read();
                robotsok = (String::from(original_hostname), robotstxt);
                robots_cache.push(robotsok.clone());
                debug!("finished, in cache");
            } else {
                robotsok = _robotsok.unwrap();
            }

            if robotsok.1.can_fetch("twentiethcrawler", &url) {
                info!("fetching {}!", url);
                let response = client.get(&url).send();

                if response.is_err() {
                    warn!("request to {} failed: {:?}", url, response);
                } else {
                    let mut response = response.unwrap();
                    let text = response.text();
                    let mut _found_urls =
                        crawl_page(&url, &response.headers(), text, &fetched_cache);

                    if _found_urls != None {
                        let mut found_urls = _found_urls.unwrap();
                        future_url_buffer.append(&mut found_urls.0.clone());

                        if found_urls.1 {
                            all_found_urls.append(&mut found_urls.0.clone());
                        }
                    }
                }
            } else {
                warn!("ignoring {} (forbidden by robots.txt)", url);
            }
        }
    }
}
