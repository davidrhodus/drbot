//! Document chunking strategies.

use crate::store::DocumentMetadata;

/// A chunk of text from a document.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Chunk content.
    pub content: String,
    /// Section name if applicable.
    pub section: Option<String>,
    /// Page number if applicable.
    pub page: Option<u32>,
    /// Start position in original document.
    pub start: usize,
    /// End position in original document.
    pub end: usize,
}

/// Chunking strategy configuration.
#[derive(Debug, Clone)]
pub struct ChunkingStrategy {
    /// Target chunk size in characters.
    pub chunk_size: usize,
    /// Overlap between chunks.
    pub overlap: usize,
    /// Respect sentence boundaries.
    pub respect_sentences: bool,
    /// Respect paragraph boundaries.
    pub respect_paragraphs: bool,
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self {
            chunk_size: 1000,
            overlap: 200,
            respect_sentences: true,
            respect_paragraphs: true,
        }
    }
}

/// Trait for document chunking.
pub trait Chunker: Send + Sync {
    /// Chunk a document into smaller pieces.
    fn chunk(&self, content: &str, metadata: DocumentMetadata) -> Vec<Chunk>;
}

/// Simple fixed-size chunker.
#[allow(dead_code)]
pub struct FixedChunker {
    strategy: ChunkingStrategy,
}

impl FixedChunker {
    #[allow(dead_code)]
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self { strategy }
    }
}

impl Default for FixedChunker {
    fn default() -> Self {
        Self::new(ChunkingStrategy::default())
    }
}

impl Chunker for FixedChunker {
    fn chunk(&self, content: &str, _metadata: DocumentMetadata) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let len = chars.len();

        if len == 0 {
            return chunks;
        }

        let mut start = 0;
        while start < len {
            let mut end = (start + self.strategy.chunk_size).min(len);

            // Try to find sentence boundary
            if self.strategy.respect_sentences && end < len {
                let search_start = end.saturating_sub(100);
                for i in (search_start..end).rev() {
                    if matches!(chars[i], '.' | '!' | '?') {
                        end = i + 1;
                        break;
                    }
                }
            }

            let chunk_content: String = chars[start..end].iter().collect();
            chunks.push(Chunk {
                content: chunk_content.trim().to_string(),
                section: None,
                page: None,
                start,
                end,
            });

            start = if end > self.strategy.overlap {
                end - self.strategy.overlap
            } else {
                end
            };

            if start >= len || end >= len {
                break;
            }
        }

        chunks
    }
}

/// Semantic chunker that respects document structure.
#[allow(dead_code)]
pub struct SemanticChunker {
    strategy: ChunkingStrategy,
}

impl SemanticChunker {
    #[allow(dead_code)]
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self { strategy }
    }
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self::new(ChunkingStrategy::default())
    }
}

impl Chunker for SemanticChunker {
    fn chunk(&self, content: &str, _metadata: DocumentMetadata) -> Vec<Chunk> {
        let mut chunks = Vec::new();

        // Split by paragraphs first
        let paragraphs: Vec<&str> = content.split("\n\n").collect();
        let mut current_chunk = String::new();
        let mut current_start = 0;

        for para in paragraphs {
            let para = para.trim();
            if para.is_empty() {
                continue;
            }

            // If adding this paragraph exceeds chunk size, save current and start new
            if !current_chunk.is_empty()
                && current_chunk.len() + para.len() + 2 > self.strategy.chunk_size
            {
                let end = current_start + current_chunk.len();
                chunks.push(Chunk {
                    content: current_chunk.clone(),
                    section: None,
                    page: None,
                    start: current_start,
                    end,
                });

                // Start new chunk with overlap from end of previous
                let overlap_start = current_chunk.len().saturating_sub(self.strategy.overlap);
                current_chunk = current_chunk[overlap_start..].to_string();
                current_start = end - (current_chunk.len());
            }

            if !current_chunk.is_empty() {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(para);
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            let end = current_start + current_chunk.len();
            chunks.push(Chunk {
                content: current_chunk,
                section: None,
                page: None,
                start: current_start,
                end,
            });
        }

        chunks
    }
}

/// Markdown-aware chunker.
#[allow(dead_code)]
pub struct MarkdownChunker {
    strategy: ChunkingStrategy,
}

impl MarkdownChunker {
    #[allow(dead_code)]
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self { strategy }
    }
}

impl Default for MarkdownChunker {
    fn default() -> Self {
        Self::new(ChunkingStrategy::default())
    }
}

impl Chunker for MarkdownChunker {
    fn chunk(&self, content: &str, _metadata: DocumentMetadata) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut current_section: Option<String> = None;
        let mut current_content = String::new();
        let mut current_start = 0;
        let mut pos = 0;

        for line in content.lines() {
            // Check for headers
            if line.starts_with('#') {
                // Save previous chunk if any
                if !current_content.is_empty() {
                    chunks.push(Chunk {
                        content: current_content.trim().to_string(),
                        section: current_section.clone(),
                        page: None,
                        start: current_start,
                        end: pos,
                    });
                    current_content = String::new();
                    current_start = pos;
                }

                // Extract section name
                current_section = Some(line.trim_start_matches('#').trim().to_string());
            }

            current_content.push_str(line);
            current_content.push('\n');
            pos += line.len() + 1;

            // Check if we've exceeded chunk size
            if current_content.len() >= self.strategy.chunk_size {
                chunks.push(Chunk {
                    content: current_content.trim().to_string(),
                    section: current_section.clone(),
                    page: None,
                    start: current_start,
                    end: pos,
                });

                // Keep overlap
                let overlap_start = current_content.len().saturating_sub(self.strategy.overlap);
                current_content = current_content[overlap_start..].to_string();
                current_start = pos - current_content.len();
            }
        }

        // Last chunk
        if !current_content.is_empty() {
            chunks.push(Chunk {
                content: current_content.trim().to_string(),
                section: current_section,
                page: None,
                start: current_start,
                end: pos,
            });
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_chunker() {
        let chunker = FixedChunker::new(ChunkingStrategy {
            chunk_size: 100,
            overlap: 20,
            respect_sentences: false,
            respect_paragraphs: false,
        });

        let content = "a".repeat(250);
        let chunks = chunker.chunk(&content, DocumentMetadata::default());

        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_semantic_chunker() {
        let chunker = SemanticChunker::new(ChunkingStrategy {
            chunk_size: 100,
            overlap: 20,
            ..Default::default()
        });

        let content = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let chunks = chunker.chunk(content, DocumentMetadata::default());

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_markdown_chunker() {
        let chunker = MarkdownChunker::default();

        let content = "# Header 1\n\nSome content.\n\n## Header 2\n\nMore content.";
        let chunks = chunker.chunk(content, DocumentMetadata::default());

        assert!(!chunks.is_empty());
        assert!(chunks.iter().any(|c| c.section.is_some()));
    }
}
