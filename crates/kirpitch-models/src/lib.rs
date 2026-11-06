// SPDX-License-Identifier: GPL-3.0-or-later

//! Language models for Kirpitch: the configuration (`kirpitch.toml`), adapters
//! for the common servers, and the extraction tasks that turn the text of a
//! datasheet into a PartSpec.
//!
//! Everything a model says is checked: answers must follow a JSON schema,
//! pins must be found in the text of the page, and the PartSpec built from
//! them goes through the same validation and review stages as one made by
//! hand. Without a configuration, Kirpitch works in no-LLM mode.

pub mod build;
pub mod client;
pub mod config;
pub mod png;
pub mod probe;
pub mod tasks;
pub mod transport;

pub use build::{BuildOptions, BuildReport, Score, assemble, build, score, template_from_answer};
pub use client::{ChatRequest, ChatResponse, Client, ModelError, Usage};
pub use config::{CONFIG_FILE, Config, ConfigError, ModelConfig, Provider, Task, is_local_url};
pub use probe::{ProbeResult, probe};
pub use tasks::{Cache, PageLines, Stats, TaskError, TextLine};
pub use transport::{CurlTransport, MockTransport, Reply, Request, Transport, TransportError};
