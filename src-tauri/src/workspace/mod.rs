//! One restaurant, one business workspace: every counter (POS terminal) of a
//! business works on the same products, stock, sales, customers, staff and
//! settings. See `docs/MULTI-COUNTER.md` in the server repository for the
//! whole design; in short:
//!
//! - the server (`pos-*.php`) holds the authoritative copy, keyed by the
//!   licence, and every counter keeps a full local copy so it works — and
//!   reports — exactly as before, offline included;
//! - shared rows carry a global id (`spec`, `local`), local changes are
//!   captured by triggers and pushed, everything new is pulled and applied
//!   (`engine`, `codec`) every few seconds (`runtime`);
//! - stock is central and moves only by signed changes, and an online sale
//!   takes its stock centrally before it is recorded (`runtime::reserve_for_sale`);
//! - product photos and the logo travel as files beside the records (`media`);
//! - staff PIN hashes travel sealed with a key only the counters hold (`pins`),
//!   and the counter's own token is kept in the OS credential store (`vault`).

pub mod client;
pub mod codec;
pub mod engine;
pub mod lan;
pub mod local;
pub mod media;
pub mod pins;
pub mod runtime;
pub mod spec;
pub mod vault;

#[cfg(test)]
mod tests;
