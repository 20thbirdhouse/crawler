extern crate env_logger;
extern crate html5ever;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate robotparser;
extern crate url;

use reqwest::Client;
use robotparser::RobotFileParser;
use url::Url;
use html5ever::tokenizer::{BufferQueue, ParseError, StartTag, TagToken, Token, TokenSink,
                           TokenSinkResult, Tokenizer};
use html5ever::tendril::{ByteTendril, Tendril};

fn get_attribute_for_elem<'a>(elem: &str) -> Option<&'a str> {
    match elem {
        "a" => Some("href"),
        "script" => Some("src"),
        "link" => Some("href"),
        "img" => Some("src"),
        "iframe" => Some("src"),
        "amp-img" => Some("src"),
        "amp-anim" => Some("src"),
        "amp-video" => Some("src"),
        "amp-audio" => Some("src"),
        "amp-iframe" => Some("src"),
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

// HACK we need a pointer to `false'
static FALSE: bool = false;
fn find_urls_in_html(
    original_url: Url,
    raw_html: String,
    fetched_cache: Vec<String>,
) -> (bool, Vec<String>) {
    struct Sink<'a> {
        original_url: Url,
        returned_vec: &'a mut Vec<String>,
        fetched_cache: Vec<String>,
        index_url: &'a bool,
        nofollow: bool,
    }

    impl<'a> TokenSink for Sink<'a> {
        type Handle = ();

        fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
            if self.nofollow {
                return TokenSinkResult::Continue;
            }

            trace!("token {:?}", token);
            match token {
                TagToken(tag) => {
                    // HACK use .trim() to convert to a &str
                    if tag.name.trim() == "meta" && (tag.kind == StartTag || tag.self_closing) {
                        let mut ok = false;
                        for attribute in tag.attrs.clone() {
                            if attribute.name.local.trim() == "name"
                                && (attribute.value == Tendril::from_slice("robots")
                                    || attribute.value == Tendril::from_slice("twentiethbot"))
                            {
                                ok = true;
                            }
                        }

                        if !ok {
                            return TokenSinkResult::Continue;
                        }

                        for attribute in tag.attrs {
                            if attribute.name.local.trim() != "content" {
                                continue;
                            }

                            for robots_command in
                                attribute.value.split(",").map(|x| x.to_lowercase())
                            {
                                debug!("found robot-command {}", robots_command);
                                match robots_command.as_str() {
                                    "nofollow" => {
                                        self.nofollow = true;
                                        return TokenSinkResult::Continue;
                                    }
                                    "noindex" => {
                                        self.index_url = &FALSE;
                                    }
                                    _ => {
                                        // Currently, other properties like noodp and noarchive are
                                        // irrelevant. We'll probably deal with them later.
                                    }
                                }
                            }
                        }
                    } else if tag.kind == StartTag && tag.attrs.len() != 0 {
                        let _attribute_name = get_attribute_for_elem(&tag.name);

                        if _attribute_name == None {
                            return TokenSinkResult::Continue;
                        }
                        let attribute_name = _attribute_name.unwrap();

                        for attribute in &tag.attrs {
                            if &attribute.name.local != attribute_name {
                                continue;
                            }

                            trace!("element {:?} found", tag);
                            add_urls_to_vec(
                                repair_suggested_url(
                                    &self.original_url,
                                    (&attribute.name.local, &attribute.value),
                                ),
                                &mut self.returned_vec,
                                &self.fetched_cache,
                            );
                        }
                    }
                }
                ParseError(error) => {
                    warn!("error parsing html for {}: {:?}", self.original_url, error);
                }
                _ => {}
            }
            return TokenSinkResult::Continue;
        }
    }

    let mut result = Vec::new();
    let index = true;
    {
        let html = Sink {
            original_url: original_url,
            returned_vec: &mut result,
            fetched_cache: fetched_cache,
            index_url: &index,
            nofollow: false,
        };

        let mut byte_tendril = ByteTendril::new();
        {
            let tendril_push_result = byte_tendril.try_push_bytes(&raw_html.into_bytes());

            if tendril_push_result.is_err() {
                warn!("error pushing bytes to tendril: {:?}", tendril_push_result);
                return (false, Vec::new());
            }
        }

        let mut queue = BufferQueue::new();
        queue.push_back(byte_tendril.try_reinterpret().unwrap());
        let mut tok = Tokenizer::new(html, std::default::Default::default()); // default default! default?
        let _feed = tok.feed(&mut queue);

        assert!(queue.is_empty());
        tok.end();
    }
    return (index, result);
}

fn repair_suggested_url(original_url: &Url, attribute: (&str, &str)) -> Option<Vec<String>> {
    let found_url = attribute.1.split("#").nth(0).unwrap().to_string();

    // NOTE Is this *really* necessary?
    if found_url.len() == 0 {
        return None;
    }

    let mut _parsed_found_url = Url::parse(&found_url);
    let mut parsed_found_url: Url;

    if !_parsed_found_url.is_err() {
        parsed_found_url = _parsed_found_url.unwrap();
    } else if found_url.starts_with(".") || found_url.starts_with("?") {
        parsed_found_url = original_url.join(&found_url).unwrap();
    } else if found_url.starts_with("/") {
        if found_url.chars().nth(1).unwrap_or(' ') != '/' {
            parsed_found_url = original_url.clone();
            parsed_found_url.set_path("/");
        } else if found_url.starts_with("//") {
            let mut modified_url = "https:".to_string();
            modified_url.push_str(&found_url);
            parsed_found_url = Url::parse(&modified_url).unwrap();
        } else {
            warn!("strange url found: {}", found_url);
            return None;
        }
    } else {
        let mut modified_url = "./".to_string();
        modified_url.push_str(&found_url);
        parsed_found_url = original_url.join(&modified_url).unwrap();
    }

    let mut _returned_vec = vec![parsed_found_url.as_str().to_string()];

    let main_domain = get_root_domain(parsed_found_url.as_str());
    if main_domain != None {
        _returned_vec.push(main_domain.unwrap());
    }

    let returned_vec: Vec<String> = _returned_vec
        .iter()
        .map(|x| {
            remove_get_params(Url::parse(x).unwrap())
                .as_str()
                .to_string()
        })
        .collect();

    return Some(returned_vec);
}

fn remove_get_params(mut url: Url) -> Url {
    lazy_static! {
        // TODO remove all these to_strings
        static ref BLOCKED_GET_PARAMS: Vec<String> = vec![
            "utm_source".to_string(),
            "utm_medium".to_string(),
            "utm_term".to_string(),
            "utm_content".to_string(),
            "utm_campaign".to_string(),
            "utm_reader".to_string(),
            "utm_place".to_string(),
            "utm_userid".to_string(),
            "utm_cid".to_string(),
            "utm_name".to_string(),
            "utm_pubreferrer".to_string(),
            "utm_swu".to_string(),
            "utm_viz_id".to_string(),
            "ga_source".to_string(),
            "ga_medium".to_string(),
            "ga_term".to_string(),
            "ga_content".to_string(),
            "ga_campaign".to_string(),
            "ga_place".to_string(),
            "yclid".to_string(),
            "_openstat".to_string(),
            "fb_action_ids".to_string(),
            "fb_action_types".to_string(),
            "fb_ref".to_string(),
            "fb_source".to_string(),
            "action_object_map".to_string(),
            "action_type_map".to_string(),
            "action_ref_map".to_string(),
            "_hsenc".to_string(),
            "mkt_tok".to_string(),
            "hmb_campaign".to_string(),
            "hmb_medium".to_string(),
            "hmb_source".to_string(),
            "lang".to_string()
        ];
    }

    let mut result = "".to_string();

    for param in url.query().unwrap_or("").replace("&amp;", "&").split("&") {
        let mut ok = true;

        for blocked_param in BLOCKED_GET_PARAMS.iter() {
            if param.starts_with(blocked_param) {
                ok = false;
            }
        }

        if ok {
            result.push_str(&param);
            result.push_str("&");
        }
    }

    if result.ends_with("&") {
        result.pop();
    }

    url.set_query(if result == "" { None } else { Some(&result) });
    return url;
}

fn crawl_page(
    url: &str,
    headers: &reqwest::header::Headers,
    _text: Result<String, reqwest::Error>,
    cache: Vec<String>,
) -> Option<(bool, Vec<String>)> {
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
        return Some(find_urls_in_html(Url::parse(url).unwrap(), text, cache));
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

        if future_urls.len() == 0 {
            panic!("no more urls???");
        }

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
                        crawl_page(&url, &response.headers(), text, fetched_cache.clone());

                    if _found_urls != None {
                        let mut found_urls = _found_urls.unwrap();
                        future_url_buffer.append(&mut found_urls.1);

                        if found_urls.0 {
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
