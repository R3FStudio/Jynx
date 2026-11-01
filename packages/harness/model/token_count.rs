use super::*;

impl ModelClient {


    pub async fn count_input_tokens(&self, messages: &[ChatMessage], tools: &Value) -> Option<usize> {
        if !self.local_compat { return None; }
        let normalized = self.request_messages(messages).ok()?;
        let mut payload = self.payload_with_reasoning(json!({
            "model": self.model, "messages": normalized, "tools": tools,
            "tool_choice": "auto", "stream": false,
        }));
        self.configure_local_reasoning(&mut payload, None).await;
        let endpoint = self.endpoint.trim_end_matches('/');
        let root = endpoint.strip_suffix("/v1").unwrap_or(endpoint);
        let missing = *self.token_count_route_missing.lock().ok()?;
        if !missing {
            let response = self.token_count_request(&format!("{root}/v1/chat/completions/input_tokens"), &payload).await?;
            if response.status().is_success() {
                let value = response.json::<Value>().await.ok()?;
                return value["input_tokens"].as_u64().and_then(|n| usize::try_from(n).ok());
            }
            if matches!(response.status().as_u16(), 404 | 405 | 501) {
                *self.token_count_route_missing.lock().ok()? = true;
            } else { return None; }
        }



        if messages.iter().any(|message| !message.images.is_empty()) { return None; }
        let response = self.token_count_request(&format!("{root}/apply-template"), &payload).await?;
        if !response.status().is_success() { return None; }
        let template = response.json::<Value>().await.ok()?;
        let prompt = template["prompt"].as_str()?;
        let response = self.token_count_request(&format!("{root}/tokenize"), &json!({
            "content": prompt, "add_special": false, "parse_special": true, "with_pieces": false,
        })).await?;
        if !response.status().is_success() { return None; }
        let value = response.json::<Value>().await.ok()?;
        Some(value["tokens"].as_array()?.len())
    }

    async fn token_count_request(&self, url: &str, payload: &Value) -> Option<reqwest::Response> {
        let mut request = self.client.post(url).json(payload).timeout(Duration::from_secs(8));
        if let Some(key) = self.api_key.as_ref().filter(|key| !key.trim().is_empty()) {
            request = request.bearer_auth(key);
        }
        request.send().await.ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

    async fn request(socket: &mut TcpStream) -> (String, Value) {
        let mut bytes = Vec::new();
        let (offset, length) = loop {
            let mut chunk = [0u8;4096];
            let count = socket.read(&mut chunk).await.unwrap(); assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(offset) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..offset]).to_ascii_lowercase();
                let length = headers.lines().find_map(|line| line.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap()).unwrap_or(0);
                break (offset + 4, length);
            }
        };
        while bytes.len() < offset + length {
            let mut chunk = [0u8;4096]; let count = socket.read(&mut chunk).await.unwrap(); assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
        }
        let line = String::from_utf8_lossy(&bytes[..offset]).lines().next().unwrap().to_owned();
        let body = if length == 0 { Value::Null } else { serde_json::from_slice(&bytes[offset..offset+length]).unwrap() };
        (line, body)
    }
    async fn respond(socket: &mut TcpStream, status: &str, body: Value) {
        let body = body.to_string();
        socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    }
    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage {role:"user".into(),content:"Synthetic test".into(),images:vec![],tool_calls:None,tool_call_id:None,name:None}]
    }

    #[tokio::test]
    async fn counts_native_template_and_tools_with_supported_reasoning() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket,_) = listener.accept().await.unwrap(); assert_eq!(request(&mut socket).await.0,"GET /props HTTP/1.1");
            respond(&mut socket,"200 OK",json!({"chat_template_caps":{"reasoning_effort_levels":["low","medium","xhigh"]}})).await;
            let (mut socket,_) = listener.accept().await.unwrap(); let (line,body) = request(&mut socket).await;
            assert_eq!(line,"POST /v1/chat/completions/input_tokens HTTP/1.1");
            assert_eq!(body["reasoning_effort"],"xhigh"); assert_eq!(body["tools"][0]["function"]["name"],"fixture_tool");
            assert_eq!(body["messages"][0]["content"],"Synthetic test");
            respond(&mut socket,"200 OK",json!({"input_tokens":1234})).await;
        });
        let client = ModelClient::new(format!("http://{address}/v1"),"fixture",None,true,"high");
        assert_eq!(client.count_input_tokens(&messages(),&json!([{"type":"function","function":{"name":"fixture_tool","parameters":{}}}])).await,Some(1234));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn older_runtime_uses_its_template_and_tokenizer_and_caches_missing_route() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket,_) = listener.accept().await.unwrap(); request(&mut socket).await; respond(&mut socket,"200 OK",json!({})).await;
            let (mut socket,_) = listener.accept().await.unwrap(); assert!(request(&mut socket).await.0.contains("/input_tokens "));
            respond(&mut socket,"404 Not Found",json!({})).await;
            for _ in 0..2 {
                let (mut socket,_) = listener.accept().await.unwrap(); let (line,body) = request(&mut socket).await;
                assert_eq!(line,"POST /apply-template HTTP/1.1"); assert!(body["tools"].is_array());
                respond(&mut socket,"200 OK",json!({"prompt":"<special>Synthetic rendered prompt"})).await;
                let (mut socket,_) = listener.accept().await.unwrap(); let (line,body) = request(&mut socket).await;
                assert_eq!(line,"POST /tokenize HTTP/1.1"); assert_eq!(body["content"],"<special>Synthetic rendered prompt");
                assert_eq!(body["parse_special"],true); assert_eq!(body["add_special"],false);
                respond(&mut socket,"200 OK",json!({"tokens":[1,2,3]})).await;
            }
        });
        let client = ModelClient::new(format!("http://{address}"),"fixture",None,true,"");
        for _ in 0..2 { assert_eq!(client.count_input_tokens(&messages(),&json!([])).await,Some(3)); }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cloud_counting_does_not_probe_a_local_endpoint() {
        let client = ModelClient::new("http://127.0.0.1:1/v1","fixture",None,false,"");
        assert_eq!(client.count_input_tokens(&messages(),&json!([])).await,None);
    }
}
