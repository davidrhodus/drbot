//! Pagination utilities for drbot.
//!
//! This crate provides:
//! - Offset-based pagination
//! - Page-based pagination
//! - Pagination metadata
//! - Link generation

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Pagination error types.
#[derive(Error, Debug)]
pub enum PaginationError {
    #[error("Invalid page number: {0}")]
    InvalidPage(i64),

    #[error("Invalid page size: {0}")]
    InvalidSize(i64),

    #[error("Invalid offset: {0}")]
    InvalidOffset(i64),
}

/// Result type for pagination operations.
pub type Result<T> = std::result::Result<T, PaginationError>;

/// Pagination request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    /// Page number (1-indexed).
    #[serde(default = "default_page")]
    pub page: u64,
    /// Items per page.
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl PaginationParams {
    /// Create new params.
    pub fn new(page: u64, per_page: u64) -> Result<Self> {
        if page == 0 {
            return Err(PaginationError::InvalidPage(page as i64));
        }
        if per_page == 0 {
            return Err(PaginationError::InvalidSize(per_page as i64));
        }
        Ok(Self { page, per_page })
    }

    /// Get offset for SQL queries.
    pub fn offset(&self) -> u64 {
        (self.page - 1) * self.per_page
    }

    /// Get limit for SQL queries.
    pub fn limit(&self) -> u64 {
        self.per_page
    }

    /// Set page.
    pub fn with_page(mut self, page: u64) -> Self {
        self.page = page.max(1);
        self
    }

    /// Set per_page.
    pub fn with_per_page(mut self, per_page: u64) -> Self {
        self.per_page = per_page.max(1);
        self
    }
}

/// Offset-based pagination params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetParams {
    /// Starting offset.
    #[serde(default)]
    pub offset: u64,
    /// Number of items.
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    20
}

impl Default for OffsetParams {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
        }
    }
}

impl OffsetParams {
    /// Create new params.
    pub fn new(offset: u64, limit: u64) -> Self {
        Self { offset, limit }
    }

    /// Convert to page params.
    pub fn to_page_params(&self) -> PaginationParams {
        let page = if self.limit > 0 {
            (self.offset / self.limit) + 1
        } else {
            1
        };
        PaginationParams {
            page,
            per_page: self.limit,
        }
    }
}

/// Pagination metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    /// Current page.
    pub current_page: u64,
    /// Items per page.
    pub per_page: u64,
    /// Total items.
    pub total_items: u64,
    /// Total pages.
    pub total_pages: u64,
    /// Has previous page.
    pub has_previous: bool,
    /// Has next page.
    pub has_next: bool,
}

impl PaginationMeta {
    /// Create new metadata.
    pub fn new(params: &PaginationParams, total_items: u64) -> Self {
        let total_pages = if params.per_page > 0 {
            (total_items + params.per_page - 1) / params.per_page
        } else {
            0
        };

        Self {
            current_page: params.page,
            per_page: params.per_page,
            total_items,
            total_pages,
            has_previous: params.page > 1,
            has_next: params.page < total_pages,
        }
    }

    /// Get first page.
    pub fn first_page(&self) -> u64 {
        1
    }

    /// Get last page.
    pub fn last_page(&self) -> u64 {
        self.total_pages.max(1)
    }

    /// Get previous page.
    pub fn previous_page(&self) -> Option<u64> {
        if self.has_previous {
            Some(self.current_page - 1)
        } else {
            None
        }
    }

    /// Get next page.
    pub fn next_page(&self) -> Option<u64> {
        if self.has_next {
            Some(self.current_page + 1)
        } else {
            None
        }
    }

    /// Get range of items on current page.
    pub fn item_range(&self) -> (u64, u64) {
        let start = ((self.current_page - 1) * self.per_page) + 1;
        let end = (start + self.per_page - 1).min(self.total_items);
        (start, end)
    }
}

/// Paginated response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paginated<T> {
    /// Items.
    pub items: Vec<T>,
    /// Pagination metadata.
    pub meta: PaginationMeta,
}

impl<T> Paginated<T> {
    /// Create new paginated response.
    pub fn new(items: Vec<T>, params: &PaginationParams, total_items: u64) -> Self {
        Self {
            items,
            meta: PaginationMeta::new(params, total_items),
        }
    }

    /// Map items.
    pub fn map<U, F>(self, f: F) -> Paginated<U>
    where
        F: FnMut(T) -> U,
    {
        Paginated {
            items: self.items.into_iter().map(f).collect(),
            meta: self.meta,
        }
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get item count.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Pagination links.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationLinks {
    /// First page URL.
    pub first: Option<String>,
    /// Previous page URL.
    pub prev: Option<String>,
    /// Current page URL.
    pub current: String,
    /// Next page URL.
    pub next: Option<String>,
    /// Last page URL.
    pub last: Option<String>,
}

/// Link builder for pagination.
pub struct LinkBuilder {
    base_url: String,
    page_param: String,
    per_page_param: String,
}

impl LinkBuilder {
    /// Create new builder.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            page_param: "page".to_string(),
            per_page_param: "per_page".to_string(),
        }
    }

    /// Set page parameter name.
    pub fn page_param(mut self, name: impl Into<String>) -> Self {
        self.page_param = name.into();
        self
    }

    /// Set per_page parameter name.
    pub fn per_page_param(mut self, name: impl Into<String>) -> Self {
        self.per_page_param = name.into();
        self
    }

    /// Build link for page.
    fn build_link(&self, page: u64, per_page: u64) -> String {
        let separator = if self.base_url.contains('?') {
            '&'
        } else {
            '?'
        };
        format!(
            "{}{}{}={}&{}={}",
            self.base_url, separator, self.page_param, page, self.per_page_param, per_page
        )
    }

    /// Build pagination links.
    pub fn build(&self, meta: &PaginationMeta) -> PaginationLinks {
        PaginationLinks {
            first: Some(self.build_link(1, meta.per_page)),
            prev: meta
                .previous_page()
                .map(|p| self.build_link(p, meta.per_page)),
            current: self.build_link(meta.current_page, meta.per_page),
            next: meta.next_page().map(|p| self.build_link(p, meta.per_page)),
            last: Some(self.build_link(meta.last_page(), meta.per_page)),
        }
    }
}

/// Page number generator for UI.
pub struct PageNumbers {
    current: u64,
    total: u64,
    window: u64,
}

impl PageNumbers {
    /// Create new generator.
    pub fn new(current: u64, total: u64, window: u64) -> Self {
        Self {
            current,
            total,
            window,
        }
    }

    /// Generate page numbers.
    pub fn generate(&self) -> Vec<PageNumber> {
        let mut pages = Vec::new();

        let start = self.current.saturating_sub(self.window).max(1);
        let end = (self.current + self.window).min(self.total);

        // Add first page and ellipsis if needed
        if start > 1 {
            pages.push(PageNumber::Page(1));
            if start > 2 {
                pages.push(PageNumber::Ellipsis);
            }
        }

        // Add window pages
        for page in start..=end {
            if page == self.current {
                pages.push(PageNumber::Current(page));
            } else {
                pages.push(PageNumber::Page(page));
            }
        }

        // Add ellipsis and last page if needed
        if end < self.total {
            if end < self.total - 1 {
                pages.push(PageNumber::Ellipsis);
            }
            pages.push(PageNumber::Page(self.total));
        }

        pages
    }
}

/// Page number entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageNumber {
    /// Regular page.
    Page(u64),
    /// Current page.
    Current(u64),
    /// Ellipsis.
    Ellipsis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params() {
        let params = PaginationParams::default();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 20);
        assert_eq!(params.offset(), 0);
        assert_eq!(params.limit(), 20);
    }

    #[test]
    fn test_offset_calculation() {
        let params = PaginationParams::default().with_page(3);
        assert_eq!(params.offset(), 40);
    }

    #[test]
    fn test_pagination_meta() {
        let params = PaginationParams::new(2, 10).unwrap();
        let meta = PaginationMeta::new(&params, 95);

        assert_eq!(meta.current_page, 2);
        assert_eq!(meta.total_pages, 10);
        assert!(meta.has_previous);
        assert!(meta.has_next);
        assert_eq!(meta.previous_page(), Some(1));
        assert_eq!(meta.next_page(), Some(3));
    }

    #[test]
    fn test_item_range() {
        let params = PaginationParams::new(2, 10).unwrap();
        let meta = PaginationMeta::new(&params, 95);

        assert_eq!(meta.item_range(), (11, 20));
    }

    #[test]
    fn test_paginated() {
        let params = PaginationParams::new(1, 2).unwrap();
        let items = vec!["a", "b"];
        let paginated = Paginated::new(items, &params, 5);

        assert_eq!(paginated.len(), 2);
        assert_eq!(paginated.meta.total_pages, 3);
    }

    #[test]
    fn test_link_builder() {
        let params = PaginationParams::new(2, 10).unwrap();
        let meta = PaginationMeta::new(&params, 50);

        let builder = LinkBuilder::new("/api/items");
        let links = builder.build(&meta);

        assert!(links.first.unwrap().contains("page=1"));
        assert!(links.prev.unwrap().contains("page=1"));
        assert!(links.current.contains("page=2"));
        assert!(links.next.unwrap().contains("page=3"));
    }

    #[test]
    fn test_page_numbers() {
        let gen = PageNumbers::new(5, 10, 2);
        let pages = gen.generate();

        assert!(pages.contains(&PageNumber::Page(1)));
        assert!(pages.contains(&PageNumber::Current(5)));
        assert!(pages.contains(&PageNumber::Page(10)));
    }

    #[test]
    fn test_offset_params() {
        let offset = OffsetParams::new(20, 10);
        let page = offset.to_page_params();

        assert_eq!(page.page, 3);
        assert_eq!(page.per_page, 10);
    }
}
