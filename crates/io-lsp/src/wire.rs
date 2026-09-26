use std::io::BufRead;

use serde_json::{Value, json};

pub(crate) struct Outgoing {
    method: &'static str,
    params: Value,
}

fn at(uri: &str, line: u32, character: u32) -> Value {
    json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}})
}

impl Outgoing {
    pub(crate) fn initialize(process: u32, uri: &str, name: &str) -> Self {
        Self {
            method: "initialize",
            params: json!({
                "processId": process,
                "rootUri": uri,
                "workspaceFolders": [{"uri": uri, "name": name}],
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                        "callHierarchy": {},
                        "references": {},
                        "hover": {"contentFormat": ["markdown", "plaintext"]},
                        "definition": {}
                    },
                    "window": {"workDoneProgress": true},
                    "workspace": {"configuration": true},
                    "experimental": {"serverStatusNotification": true}
                },
                "initializationOptions": {"checkOnSave": false, "cachePriming": {"enable": false}}
            }),
        }
    }

    pub(crate) fn initialized() -> Self {
        Self {
            method: "initialized",
            params: json!({}),
        }
    }

    pub(crate) fn shutdown() -> Self {
        Self {
            method: "shutdown",
            params: Value::Null,
        }
    }

    pub(crate) fn exit() -> Self {
        Self {
            method: "exit",
            params: Value::Null,
        }
    }

    pub(crate) fn document_symbol(uri: &str) -> Self {
        Self {
            method: "textDocument/documentSymbol",
            params: json!({"textDocument": {"uri": uri}}),
        }
    }

    pub(crate) fn prepare_call_hierarchy(uri: &str, line: u32, character: u32) -> Self {
        Self {
            method: "textDocument/prepareCallHierarchy",
            params: at(uri, line, character),
        }
    }

    pub(crate) fn outgoing_calls(item: &Value) -> Self {
        Self {
            method: "callHierarchy/outgoingCalls",
            params: json!({"item": item}),
        }
    }

    pub(crate) fn incoming_calls(item: &Value) -> Self {
        Self {
            method: "callHierarchy/incomingCalls",
            params: json!({"item": item}),
        }
    }

    pub(crate) fn references(uri: &str, line: u32, character: u32) -> Self {
        Self {
            method: "textDocument/references",
            params: json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}, "context": {"includeDeclaration": false}}),
        }
    }

    pub(crate) fn hover(uri: &str, line: u32, character: u32) -> Self {
        Self {
            method: "textDocument/hover",
            params: at(uri, line, character),
        }
    }

    pub(crate) fn definition(uri: &str, line: u32, character: u32) -> Self {
        Self {
            method: "textDocument/definition",
            params: at(uri, line, character),
        }
    }

    pub(crate) fn request(self, id: u64) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": self.method, "params": self.params})
    }

    pub(crate) fn notification(self) -> Value {
        json!({"jsonrpc": "2.0", "method": self.method, "params": self.params})
    }
}

pub(crate) fn reply(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub(crate) fn frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

pub(crate) fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            if length > 0 {
                break;
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

pub(crate) fn answer_id(message: &Value) -> Option<u64> {
    if message.get("method").is_some() {
        return None;
    }
    message.get("id").and_then(Value::as_u64)
}

pub(crate) fn outcome(mut message: Value) -> Result<Value, String> {
    if let Some(error) = message.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .to_owned());
    }
    Ok(message.get_mut("result").map_or(Value::Null, Value::take))
}

pub(crate) enum ProgressKind {
    Begin,
    End,
    Other,
}

pub(crate) enum Notice {
    Progress(ProgressKind),
    Status(Option<bool>),
    Configuration { id: Value, count: usize },
    Request { id: Value },
    Ignored,
}

pub(crate) fn notice(message: &Value) -> Notice {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = || message.get("id").cloned().unwrap_or(Value::Null);
    match method {
        "$/progress" => Notice::Progress(
            match message
                .pointer("/params/value/kind")
                .and_then(Value::as_str)
            {
                Some("begin") => ProgressKind::Begin,
                Some("end") => ProgressKind::End,
                _ => ProgressKind::Other,
            },
        ),
        "experimental/serverStatus" => Notice::Status(
            message
                .pointer("/params/quiescent")
                .and_then(Value::as_bool),
        ),
        "workspace/configuration" => Notice::Configuration {
            id: id(),
            count: message
                .pointer("/params/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        },
        _ if message.get("id").is_some() => Notice::Request { id: id() },
        _ => Notice::Ignored,
    }
}

pub(crate) fn nulls(count: usize) -> Value {
    Value::Array(vec![Value::Null; count])
}

pub(crate) struct WireSymbol {
    pub(crate) name: String,
    pub(crate) kind: u64,
    pub(crate) selection_line: u64,
    pub(crate) selection_character: u64,
    pub(crate) end_line: u64,
    pub(crate) end_at_line_start: bool,
    pub(crate) children: Vec<WireSymbol>,
}

fn number(value: &Value, pointer: &str) -> u64 {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(crate) fn symbol(value: &Value) -> WireSymbol {
    WireSymbol {
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        kind: number(value, "/kind"),
        selection_line: number(value, "/selectionRange/start/line"),
        selection_character: number(value, "/selectionRange/start/character"),
        end_line: number(value, "/range/end/line"),
        end_at_line_start: value
            .pointer("/range/end/character")
            .and_then(Value::as_i64)
            == Some(0),
        children: value
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(symbol)
            .collect(),
    }
}

pub(crate) fn symbols(answer: &Value) -> Vec<WireSymbol> {
    answer
        .as_array()
        .into_iter()
        .flatten()
        .map(symbol)
        .collect()
}

pub(crate) fn items(answer: &Value) -> Vec<Value> {
    answer.as_array().cloned().unwrap_or_default()
}

pub(crate) struct WireLocation {
    pub(crate) uri: String,
    pub(crate) line: u64,
}

pub(crate) enum LocationShape {
    Outgoing,
    Incoming,
    Reference,
}

impl LocationShape {
    fn pointers(&self) -> (&'static str, &'static str) {
        match self {
            Self::Outgoing => ("/to/uri", "/to/selectionRange/start/line"),
            Self::Incoming => ("/from/uri", "/from/selectionRange/start/line"),
            Self::Reference => ("/uri", "/range/start/line"),
        }
    }
}

pub(crate) fn locations(answer: &Value, shape: &LocationShape) -> Vec<WireLocation> {
    let (uri, line) = shape.pointers();
    answer
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(WireLocation {
                uri: entry.pointer(uri)?.as_str()?.to_owned(),
                line: entry.pointer(line)?.as_u64()?,
            })
        })
        .collect()
}

pub(crate) fn hover_parts(answer: &Value) -> Vec<String> {
    let value_of = |entry: &Value| {
        entry
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match answer.get("contents") {
        Some(Value::Array(parts)) => parts
            .iter()
            .map(|part| part.as_str().map_or_else(|| value_of(part), str::to_owned))
            .collect(),
        Some(Value::String(text)) => vec![text.clone()],
        Some(other) => vec![value_of(other)],
        None => vec![value_of(&Value::Null)],
    }
}

pub(crate) struct WireDefinition {
    pub(crate) uri: String,
    pub(crate) line: u64,
    pub(crate) character: u64,
}

pub(crate) fn definition(answer: &Value) -> Option<WireDefinition> {
    let link = match answer.as_array() {
        Some(list) => list.first()?,
        None => answer,
    };
    let (uri, range) = if link.get("targetUri").is_some() {
        ("/targetUri", "/targetSelectionRange/start")
    } else {
        ("/uri", "/range/start")
    };
    let start = link.pointer(range);
    Some(WireDefinition {
        uri: link.pointer(uri)?.as_str()?.to_owned(),
        line: start?.get("line")?.as_u64()?,
        character: start?.get("character")?.as_u64()?,
    })
}
