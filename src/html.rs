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
                        meta.push(((&attribute.name.local).to_string(), (&attribute.value).to_string()));
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
                warn!("error parsing html for {}: {:?}", original_url, error);
            }
            _ => {}
        }
    }

    return Some((index_url, found_urls, "html".to_string(), meta));
}
