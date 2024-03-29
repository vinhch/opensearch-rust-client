/*
 * Licensed to Elasticsearch B.V. under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch B.V. licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *	http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * The OpenSearch Contributors require contributions made to
 * this file be licensed under the Apache-2.0 license or a
 * compatible open source license.
 *
 * Modifications Copyright OpenSearch Contributors. See
 * GitHub history for details.
 */

pub mod common;
use common::*;
use hyper::Method;

use crate::common::{client::index_documents, server::MockServer};
use bytes::Bytes;
use opensearch::{
    http::{
        headers::{
            HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE, DEFAULT_ACCEPT,
            DEFAULT_CONTENT_TYPE, X_OPAQUE_ID,
        },
        StatusCode,
    },
    params::TrackTotalHits,
    SearchParts,
};
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::test]
async fn default_user_agent_content_type_accept_headers() -> anyhow::Result<()> {
    let mut server = MockServer::start()?;

    let _ = server.client().ping().send().await?;

    let request = server.received_request().await?;

    assert_eq!(request.header("user-agent"), Some(DEFAULT_USER_AGENT));
    assert_eq!(request.header("content-type"), Some(DEFAULT_CONTENT_TYPE));
    assert_eq!(request.header("accept"), Some(DEFAULT_ACCEPT));

    Ok(())
}

#[tokio::test]
async fn default_header() -> anyhow::Result<()> {
    let mut server = MockServer::start()?;

    let client = server.client_with(|b| {
        b.header(
            HeaderName::from_static(X_OPAQUE_ID),
            HeaderValue::from_static("foo"),
        )
    });

    let _ = client.ping().send().await?;

    let request = server.received_request().await?;

    assert_eq!(request.header("x-opaque-id"), Some("foo"));

    Ok(())
}

#[tokio::test]
async fn override_default_header() -> anyhow::Result<()> {
    let mut server = MockServer::start()?;

    let client = server.client_with(|b| {
        b.header(
            HeaderName::from_static(X_OPAQUE_ID),
            HeaderValue::from_static("foo"),
        )
    });

    let _ = client
        .ping()
        .header(
            HeaderName::from_static(X_OPAQUE_ID),
            HeaderValue::from_static("bar"),
        )
        .send()
        .await?;

    let request = server.received_request().await?;

    assert_eq!(request.header("x-opaque-id"), Some("bar"));

    Ok(())
}

#[tokio::test]
async fn x_opaque_id_header() -> anyhow::Result<()> {
    let mut server = MockServer::start()?;

    let _ = server
        .client()
        .ping()
        .header(
            HeaderName::from_static(X_OPAQUE_ID),
            HeaderValue::from_static("foo"),
        )
        .send()
        .await?;

    let request = server.received_request().await?;

    assert_eq!(request.header("x-opaque-id"), Some("foo"));

    Ok(())
}

#[tokio::test]
async fn uses_global_request_timeout() -> anyhow::Result<()> {
    let server = MockServer::builder()
        .response_delay(Duration::from_secs(1))
        .start()?;

    let client = server.client_with(|b| b.timeout(Duration::from_millis(500)));

    let response = client.ping().send().await;

    match response {
        Ok(_) => panic!("Expected timeout error, but response received"),
        Err(e) => assert!(e.is_timeout(), "Expected timeout error, but was {:?}", e),
    }

    Ok(())
}

#[tokio::test]
async fn uses_call_request_timeout() -> anyhow::Result<()> {
    let server = MockServer::builder()
        .response_delay(Duration::from_secs(1))
        .start()?;

    let client = server.client_with(|b| b.timeout(Duration::from_secs(2)));

    let response = client
        .ping()
        .request_timeout(Duration::from_millis(500))
        .send()
        .await;

    match response {
        Ok(_) => panic!("Expected timeout error, but response received"),
        Err(e) => assert!(e.is_timeout(), "Expected timeout error, but was {:?}", e),
    }

    Ok(())
}

#[tokio::test]
async fn call_request_timeout_supersedes_global_timeout() -> anyhow::Result<()> {
    let server = MockServer::builder()
        .response_delay(Duration::from_secs(1))
        .start()?;

    let client = server.client_with(|b| b.timeout(Duration::from_millis(500)));

    let response = client
        .ping()
        .request_timeout(Duration::from_secs(2))
        .send()
        .await;

    assert!(
        response.is_ok(),
        "Expected response, but was: {:?}",
        response
    );

    Ok(())
}

#[tokio::test]
async fn deprecation_warning_headers() -> anyhow::Result<()> {
    let client = client::create();
    let _ = index_documents(&client).await?;
    let response = client
        .search(SearchParts::None)
        .body(json! {
            {
              "aggs": {
                "titles": {
                  "terms": {
                    "field": "title.keyword",
                    "order": [{
                      "_term": "asc"
                    }]
                  }
                }
              },
              "query": {
                "function_score": {
                  "functions": [{
                    "random_score": {
                      "seed": 1337
                    }
                  }],
                  "query": {
                    "match_all": {}
                  }
                }
              },
              "size": 0
            }
        })
        .send()
        .await?;

    let warnings = response.warning_headers().collect::<Vec<&str>>();
    assert!(!warnings.is_empty());
    assert!(warnings
        .iter()
        .any(|&w| w.contains("Deprecated aggregation order key")));

    Ok(())
}

#[tokio::test]
async fn serialize_querystring() -> anyhow::Result<()> {
    let mut server = MockServer::start()?;

    let _ = server
        .client()
        .search(SearchParts::None)
        .pretty(true)
        .filter_path(&["took", "_shards"])
        .track_total_hits(TrackTotalHits::Count(100_000))
        .q("title:OpenSearch")
        .send()
        .await?;

    let request = server.received_request().await?;
    assert_eq!(request.method(), Method::GET);
    assert_eq!(request.path(), "/_search");
    assert_eq!(
        request.query(),
        Some("filter_path=took%2C_shards&pretty=true&q=title%3AOpenSearch&track_total_hits=100000")
    );

    Ok(())
}

#[tokio::test]
async fn search_with_body() -> anyhow::Result<()> {
    let client = client::create();
    let _ = index_documents(&client).await?;
    let response = client
        .search(SearchParts::None)
        .body(json!({
            "query": {
                "match_all": {}
            }
        }))
        .allow_no_indices(true)
        .send()
        .await?;

    let expected_url = {
        let mut addr = client::cluster_addr();
        if !addr.ends_with('/') {
            addr.push('/');
        }
        let mut url = url::Url::parse(addr.as_str())?;
        url.set_username("").unwrap();
        url.set_password(None).unwrap();
        url.join("_search?allow_no_indices=true")?
    };

    if let Some(c) = response.content_length() {
        assert!(c > 0)
    };

    assert_eq!(response.url(), &expected_url);
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.method(), opensearch::http::Method::Post);
    let debug = format!("{:?}", &response);
    assert!(debug.contains("method"));
    assert!(debug.contains("status_code"));
    assert!(debug.contains("headers"));
    let response_body = response.json::<Value>().await?;
    assert!(response_body["took"].as_i64().is_some());

    Ok(())
}

#[tokio::test]
async fn search_with_no_body() -> anyhow::Result<()> {
    let client = client::create();
    let _ = index_documents(&client).await?;
    let response = client
        .search(SearchParts::None)
        .pretty(true)
        .q("title:OpenSearch")
        .send()
        .await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.method(), opensearch::http::Method::Get);
    let response_body = response.json::<Value>().await?;
    assert!(response_body["took"].as_i64().is_some());

    for hit in response_body["hits"]["hits"].as_array().unwrap() {
        assert!(hit["_source"]["title"].as_str().is_some());
    }

    Ok(())
}

#[tokio::test]
async fn read_response_as_bytes() -> anyhow::Result<()> {
    let client = client::create();
    let _ = index_documents(&client).await?;
    let response = client
        .search(SearchParts::None)
        .pretty(true)
        .q("title:OpenSearch")
        .send()
        .await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.method(), opensearch::http::Method::Get);

    let response: Bytes = response.bytes().await?;
    let json: Value = serde_json::from_slice(&response).unwrap();

    assert!(json["took"].as_i64().is_some());

    for hit in json["hits"]["hits"].as_array().unwrap() {
        assert!(hit["_source"]["title"].as_str().is_some());
    }

    Ok(())
}

#[tokio::test]
async fn cat_health_format_json() -> anyhow::Result<()> {
    let client = client::create();
    let response = client
        .cat()
        .health()
        .format("json")
        .pretty(true)
        .send()
        .await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with(DEFAULT_CONTENT_TYPE));
    let _response_body = response.json::<Value>().await?;

    Ok(())
}

#[tokio::test]
async fn cat_health_header_json() -> anyhow::Result<()> {
    let client = client::create();
    let response = client
        .cat()
        .health()
        .header(ACCEPT, HeaderValue::from_static(DEFAULT_ACCEPT))
        .pretty(true)
        .send()
        .await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with(DEFAULT_CONTENT_TYPE));
    let _response_body = response.json::<Value>().await?;

    Ok(())
}

#[tokio::test]
async fn cat_health_text() -> anyhow::Result<()> {
    let client = client::create();
    let response = client.cat().health().pretty(true).send().await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/plain"));
    let _response_body = response.text().await?;

    Ok(())
}

#[tokio::test]
async fn clone_search_with_body() -> anyhow::Result<()> {
    let client = client::create();
    let _ = index_documents(&client).await?;
    let base_request = client.search(SearchParts::None);

    let request_clone = base_request.clone().q("title:OpenSearch").size(1);

    let _request = base_request
        .body(json!({
            "query": {
                "match_all": {}
            }
        }))
        .allow_no_indices(true);

    let response = request_clone.send().await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    let response_body = response.json::<Value>().await?;

    assert_eq!(response_body["hits"]["hits"].as_array().unwrap().len(), 1);

    Ok(())
}

#[tokio::test]
async fn byte_slice_body() -> anyhow::Result<()> {
    let client = client::create();
    let body = b"{\"query\":{\"match_all\":{}}}";

    let response = client
        .send(
            opensearch::http::Method::Post,
            SearchParts::None.url().as_ref(),
            HeaderMap::new(),
            Option::<&Value>::None,
            Some(body.as_ref()),
            None,
        )
        .await?;

    assert_eq!(response.status_code(), StatusCode::OK);
    let _response_body = response.json::<Value>().await?;

    Ok(())
}
