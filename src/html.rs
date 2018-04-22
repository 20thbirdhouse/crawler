use url::Url;
use html5ever::tokenizer::*;
use html5ever::tendril::{ByteTendril, Tendril};
use url_utils::*;
use std;

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

struct HtmlTokenSink<'a>(&'a mut Vec<Token>);

impl<'a> TokenSink for HtmlTokenSink<'a> {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        self.0.push(token);
        return TokenSinkResult::Continue;
    }
}

pub fn find_urls_in_html(
    original_url: Url,
    raw_html: String,
    fetched_cache: Vec<String>,
) -> Option<(bool, Vec<String>, String, Vec<(String, String)>)> {
    let mut result = Vec::new();
    let mut index_url = true;
    let mut found_urls = Vec::new();
    let mut meta: Vec<(String, String)> = Vec::new();

    {
        let html = HtmlTokenSink(&mut result);

        let mut byte_tendril = ByteTendril::new();
        {
            let tendril_push_result = byte_tendril.try_push_bytes(&raw_html.into_bytes());

            if tendril_push_result.is_err() {
                warn!("error pushing bytes to tendril: {:?}", tendril_push_result);
                return None;
            }
        }

        let mut queue = BufferQueue::new();
        queue.push_back(byte_tendril.try_reinterpret().unwrap());
        let mut tok = Tokenizer::new(html, std::default::Default::default()); // default default! default?
        let _feed = tok.feed(&mut queue);

        assert!(queue.is_empty());
        tok.end();
    }

    for token in result {
        trace!("token {:?}", token);

        match token {
            TagToken(tag) => {
                if &tag.name == "meta" && (tag.kind == StartTag || tag.self_closing) {
                    let mut ok = false;

                    for attribute in tag.attrs.clone() {
                        meta.push((
                            (&attribute.name.local).to_string(),
                            (&attribute.value).to_string(),
                        ));
                        if &attribute.name.local == "name"
                            && (attribute.value == Tendril::from_slice("robots")
                                || attribute.value == Tendril::from_slice("twentiethbot"))
                        {
                            ok = true;
                        }
                    }

                    if !ok {
                        continue;
                    }

                    for attribute in tag.attrs {
                        if &attribute.name.local != "content" {
                            continue;
                        }

                        for robots_command in attribute.value.split(",").map(|x| x.to_lowercase()) {
                            debug!("found robot-command {}", robots_command);

                            match robots_command.as_str() {
                                "nofollow" => {
                                    return None;
                                }
                                "noindex" => {
                                    index_url = true;
                                }
                                _ => {}
                            }
                        }
                    }
                } else if tag.kind == StartTag && tag.attrs.len() != 0 {
                    let attribute_name = get_attribute_for_elem(&tag.name);

                    if attribute_name == None {
                        continue;
                    }

                    for attribute in &tag.attrs {
                        if &attribute.name.local != attribute_name.unwrap() {
                            continue;
                        }

                        trace!("element {:?} found", tag);
                        add_urls_to_vec(
                            repair_suggested_url(
                                &original_url,
                                (&attribute.name.local, &attribute.value),
                            ),
                            &mut found_urls,
                            &fetched_cache,
                        );
                    }
                }
            }
            ParseError(error) => {
                debug!("error parsing html for {}: {:?}", original_url, error);
            }
            _ => {}
        }
    }

    return Some((index_url, found_urls, "html".to_string(), meta));
}

#[cfg(test)]
mod tests {
    use html::*;

    #[test]
    fn _get_attribute_for_elem() {
        assert_eq!(get_attribute_for_elem("a"), Some("href"));
        assert_eq!(get_attribute_for_elem("script"), Some("src"));
        assert_eq!(get_attribute_for_elem("link"), Some("href"));
        assert_eq!(get_attribute_for_elem("img"), Some("src"));
        assert_eq!(get_attribute_for_elem("iframe"), Some("src"));
        assert_eq!(get_attribute_for_elem("amp-img"), Some("src"));
        assert_eq!(get_attribute_for_elem("amp-anim"), Some("src"));
        assert_eq!(get_attribute_for_elem("amp-video"), Some("src"));
        assert_eq!(get_attribute_for_elem("amp-audio"), Some("src"));
        assert_eq!(get_attribute_for_elem("amp-iframe"), Some("src"));
        assert_eq!(get_attribute_for_elem("p"), None);
    }

    #[test]
    fn _html_token_sink() {
        let mut result = Vec::new();

        {
            let mut sink = HtmlTokenSink(&mut result);

            assert_eq!(sink.0.len(), 0);
            assert_eq!(sink.process_token(EOFToken, 0), TokenSinkResult::Continue);
            assert_eq!(sink.0.len(), 1);
        }
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], EOFToken);
    }

    #[test]
    fn _find_urls_in_html() {
        #[allow(non_snake_case)]
        fn S(inp: &str) -> String {
            inp.to_string()
        }

        let orig = Url::parse("https://google.com/").unwrap();
        assert_eq!(
            find_urls_in_html(
                orig.clone(),
                S("<a href='news'></a><a href='gmail'></a>"),
                Vec::new()
            ),
            Some((
                true,
                vec![S("https://google.com/news"), S("https://google.com/gmail")],
                S("html"),
                Vec::new()
            ))
        );
        assert_eq!(
            find_urls_in_html(
                orig.clone(),
                S("<meta name='terminator' content='destroy' />"),
                Vec::new()
            ),
            Some((
                true,
                Vec::new(),
                S("html"),
                vec![(S("terminator"), S("destroy"))]
            ))
        );
        assert_eq!(
            find_urls_in_html(
                orig.clone(),
                S("<a href='news'></a><a href='gmail'></a>"),
                vec![S("https://google.com/news")]
            ),
            Some((
                true,
                vec![S("https://google.com/gmail")],
                S("html"),
                Vec::new()
            ))
        );
    }
}
