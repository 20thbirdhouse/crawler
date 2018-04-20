use url::Url;

pub fn repair_suggested_url(original_url: &Url, attribute: (&str, &str)) -> Option<Vec<String>> {
    let found_url = attribute.1.split("#").nth(0).unwrap().to_string();

    // NOTE is this necessary?
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

static BLOCKED_GET_PARAMS: [&str; 34] = [
    "utm_source",
    "utm_medium",
    "utm_term",
    "utm_content",
    "utm_campaign",
    "utm_reader",
    "utm_place",
    "utm_userid",
    "utm_cid",
    "utm_name",
    "utm_pubreferrer",
    "utm_swu",
    "utm_viz_id",
    "ga_source",
    "ga_medium",
    "ga_term",
    "ga_content",
    "ga_campaign",
    "ga_place",
    "yclid",
    "_openstat",
    "fb_action_ids",
    "fb_action_types",
    "fb_ref",
    "fb_source",
    "action_object_map",
    "action_type_map",
    "action_ref_map",
    "_hsenc",
    "mkt_tok",
    "hmb_campaign",
    "hmb_medium",
    "hmb_source",
    "lang",
];

pub fn remove_get_params(mut url: Url) -> Url {
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

pub fn add_urls_to_vec(urls: Option<Vec<String>>, into: &mut Vec<String>, cache: &Vec<String>) {
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

pub fn get_root_domain(url: &str) -> Option<String> {
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

pub fn check_if_is_in_url_list(object: &str, array: &Vec<String>) -> bool {
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

// =====================================================================[Tests]
#[cfg(test)]
mod tests {
    use url_utils::*;

    #[test]
    fn _remove_get_params() {
        for param in BLOCKED_GET_PARAMS.to_vec() {
            let url = Url::parse(&format!("https://test.domain/test?{}=1", param)).unwrap();
            assert_eq!("https://test.domain/test", remove_get_params(url).as_str());
        }
    }

    #[test]
    fn _check_if_is_in_url_list() {
        let mut fake_vec: Vec<String> = vec!["0".to_string()];

        for i in 1..100 {
            assert!(!check_if_is_in_url_list(&(i - 1).to_string(), &fake_vec));
            fake_vec.push(i.to_string());
        }
    }

    #[test]
    fn _get_root_domain() {
        assert_eq!(
            get_root_domain("https://test.test.domain/").unwrap(),
            "https://test.domain/"
        );
        assert_eq!(
            get_root_domain("https://test.test.test.domain/").unwrap(),
            "https://test.domain/"
        );
    }

    #[test]
    fn _add_url_to_vec() {
        let mut fake_vec: Vec<String> = Vec::new();

        add_urls_to_vec(
            Some(vec!["https://google.com".to_string()]),
            &mut fake_vec,
            &Vec::new(),
        );
        assert_eq!(fake_vec.len(), 1);
        add_urls_to_vec(
            Some(vec!["https://google.com".to_string()]),
            &mut fake_vec,
            &Vec::new(),
        );
        assert_eq!(fake_vec.len(), 1);
        add_urls_to_vec(
            Some(vec!["https://google.gl".to_string()]),
            &mut fake_vec,
            &Vec::new(),
        );
        assert_eq!(fake_vec.len(), 2);

        let fake_cache: Vec<String> = vec!["https://google.pl".to_string()];
        add_urls_to_vec(
            Some(vec!["https://google.pl".to_string()]),
            &mut fake_vec,
            &fake_cache,
        );
        assert_eq!(fake_vec.len(), 2);
    }

    #[test]
    fn _repair_suggested_url() {
        fn url(input: &str) -> Url {
            Url::parse(input).unwrap()
        }

        assert_eq!(
            repair_suggested_url(&url("https://google.com"), ("href", "main")),
            Some(vec!["https://google.com/main".to_string()])
        );
        assert_eq!(
            repair_suggested_url(&url("https://google.com/test"), ("href", "/main")),
            Some(vec!["https://google.com/main".to_string()])
        );
        assert_eq!(
            repair_suggested_url(&url("https://google.com"), ("href", "./main")),
            Some(vec!["https://google.com/main".to_string()])
        );
        assert_eq!(
            repair_suggested_url(&url("https://google.com"), ("href", "//bing.com")),
            Some(vec!["https://bing.com".to_string()])
        );
        assert_eq!(
            repair_suggested_url(
                &url("https://google.com"),
                ("href", "https://its.goggle.com")
            ),
            Some(vec![
                "https://its.goggle.com".to_string(),
                "https://goggle.com".to_string(),
            ])
        );
    }
}
