extern crate env_logger;
extern crate htmlstream;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate robotparser;
extern crate url;

use reqwest::Client;
use robotparser::RobotFileParser;
use url::Url;

fn get_attribute_for_elem(elem: &str) -> String {
    match elem {
        "a" => String::from("href"),
        "script" => String::from("src"),
        "link" => String::from("href"),
        "img" => String::from("src"),
        _ => String::from("NO_OPERATION"),
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
        let mut ret = String::from(parsed_url.scheme());
        ret.push_str("://");
        ret.push_str(hostname.unwrap());
        ret.push('/');
        debug!("found {} to be main domain!", ret);
        return Some(String::from(ret));
    } else {
        debug!("{} is main domain, ignore", hostname.unwrap());
        return None;
    }
}

fn find_urls_in_url(client: &Client, url: &String, fetched_cache: &Vec<String>) -> Vec<String> {
    if url.contains(".js") {
        return Vec::new();
    }

    let mut returned_vec: Vec<String> = Vec::new();

    for (_pos, tag) in htmlstream::tag_iter(&client.get(url).send().unwrap().text().unwrap()) {
        if tag.state == htmlstream::HTMLTagState::Opening {
            let attribute_name = get_attribute_for_elem(&tag.name);
            if attribute_name != String::from("NO_OPERATION") && tag.attributes != "" {
                for attribute_set in tag.attributes.split(" ") {
                    if attribute_set.contains("=") {
                        let mut attribute_splitted = attribute_set.split("=\"");
                        if String::from(attribute_splitted.next().unwrap()) == attribute_name {
                            let mut found_url = String::from(attribute_splitted.next().unwrap())
                                .replace("\n", "")
                                .split("#")
                                .nth(0)
                                .unwrap()
                                .to_string();
                            for found_url_part in attribute_splitted.next() {
                                found_url.push_str(found_url_part);
                            }
                            found_url.pop(); // Remove final quote

                            if found_url.chars().nth(0) == None
                                || found_url.chars().nth(0).unwrap() == '?'
                            {
                                found_url = String::from("NO_OPERATION");
                            } else if found_url.chars().nth(0).unwrap().to_string() == "." {
                                let parsed_main_url = Url::parse(url).unwrap();
                                found_url = String::from(
                                    parsed_main_url.join(&found_url).unwrap().as_str(),
                                );
                            } else if found_url.chars().nth(0).unwrap().to_string() == "/" {
                                let mut modified_url = String::from("");

                                if found_url == "/" {
                                    let parsed_url = Url::parse(url).unwrap();
                                    modified_url.push_str(parsed_url.scheme());
                                    modified_url.push_str("://");
                                    modified_url.push_str(parsed_url.host_str().unwrap());
                                } else if found_url.chars().nth(1).unwrap().to_string() == "//" {
                                    modified_url.push_str("https:");
                                } else {
                                    let parsed_url = Url::parse(url).unwrap();
                                    modified_url.push_str(parsed_url.scheme());
                                    modified_url.push_str("://");
                                    modified_url.push_str(parsed_url.host_str().unwrap());
                                }
                                modified_url.push_str(&found_url);
                                found_url = modified_url;
                            }
                            if found_url != "NO_OPERATION"
                                && check_if_is_in_url_list(&found_url, &returned_vec)
                                && check_if_is_in_url_list(&found_url, &fetched_cache)
                            {
                                trace!("found url in {} => {}", url, found_url);
                                returned_vec.push(found_url.clone());

                                let main_domain = get_root_domain(&found_url.clone());

                                if main_domain != None
                                    && check_if_is_in_url_list(
                                        &main_domain.clone().unwrap(),
                                        &returned_vec,
                                    ) {
                                    returned_vec.push(main_domain.unwrap());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    return returned_vec;
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

    info!("fetching {}!", &std::env::args().nth(1).unwrap());
    let mut future_url_buffer: Vec<String> =
        find_urls_in_url(&client, &std::env::args().nth(1).unwrap(), &Vec::new());
    let mut robots_cache: Vec<(String, RobotFileParser)> = Vec::new();
    let mut fetched_cache: Vec<String> = Vec::new();

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
                &mut future_url_buffer.append(&mut find_urls_in_url(&client, &url, &fetched_cache));
            } else {
                warn!("ignoring {} (forbidden by robots.txt)", url);
            }
        }
    }
}
