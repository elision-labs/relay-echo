use std::collections::HashMap;
use std::fmt;
use protobuf::Message;
use protobuf::ProtobufEnum;

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum FetchResponse_StatusCode {
  OK = 0,
  NOT_FOUND = 1,
  ERROR = 2,
}

impl ProtobufEnum for FetchResponse_StatusCode {
  fn value(&self) -> i32 {
    *self as i32
  }

  fn from_i32(value: i32) -> Option<FetchResponse_StatusCode> {
    match value {
      0 => Some(FetchResponse_StatusCode::OK),
      1 => Some(FetchResponse_StatusCode::NOT_FOUND),
      2 => Some(FetchResponse_StatusCode::ERROR),
      _ => None,
    }
  }
}

#[derive(Clone, PartialEq, ::protobuf::Message)]
pub struct FetchRequest {
  pub identifier: String,
}

impl FetchRequest {
  pub fn new() -> FetchRequest {
    FetchRequest {
      identifier: String::new(),
    }
  }
}

#[derive(Clone, PartialEq, ::protobuf::Message)]
pub struct FetchResponse {
  pub status: FetchResponse_StatusCode,
  pub data: Vec<u8>,
}

impl FetchResponse {
  pub fn new() -> FetchResponse {
    FetchResponse {
      status: FetchResponse_StatusCode::OK,
      data: Vec::new(),
    }
  }
}

impl fmt::Display for FetchResponse_StatusCode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      FetchResponse_StatusCode::OK => "OK",
      FetchResponse_StatusCode::NOT_FOUND => "NOT_FOUND",
      FetchResponse_StatusCode::ERROR => "ERROR",
    })
  }
}
