//! MARC record parsing and translation
//!
//! This module provides functionality to parse MARC21 and UNIMARC records
//! and translate them into the internal Item structure.

pub mod translator;

pub use z3950_rs::marc_rs::{Record as MarcRecord, MarcFormat};





