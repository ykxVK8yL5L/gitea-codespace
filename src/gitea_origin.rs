use axum::http::HeaderMap;

pub fn infer_gitea_base_url(headers: &HeaderMap) -> Option<String> {
    header_origin(headers).or_else(|| header_referer_origin(headers))
}

fn header_origin(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("origin")?.to_str().ok()?;
    normalize_origin(value)
}

fn header_referer_origin(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("referer")?.to_str().ok()?;
    let origin_end = value.find("://").and_then(|scheme_end| {
        value[scheme_end + 3..]
            .find('/')
            .map(|path_start| scheme_end + 3 + path_start)
    });

    match origin_end {
        Some(index) => normalize_origin(&value[..index]),
        None => normalize_origin(value),
    }
}

fn normalize_origin(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn infers_from_origin_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "origin",
            HeaderValue::from_static("http://192.168.1.168:20080"),
        );

        assert_eq!(
            infer_gitea_base_url(&headers).as_deref(),
            Some("http://192.168.1.168:20080")
        );
    }

    #[test]
    fn infers_from_referer_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "referer",
            HeaderValue::from_static("http://192.168.1.168:20080/root/mycode"),
        );

        assert_eq!(
            infer_gitea_base_url(&headers).as_deref(),
            Some("http://192.168.1.168:20080")
        );
    }
}
