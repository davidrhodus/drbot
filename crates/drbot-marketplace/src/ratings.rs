//! Rating and review system for marketplace items.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Rating for a marketplace item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rating {
    /// Rating ID.
    pub id: Uuid,
    /// Item ID.
    pub item_id: Uuid,
    /// User ID.
    pub user_id: String,
    /// Rating value (1-5).
    pub value: u8,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl Rating {
    /// Create a new rating.
    pub fn new(item_id: Uuid, user_id: &str, value: u8) -> Self {
        let value = value.min(5).max(1);
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            item_id,
            user_id: user_id.to_string(),
            value,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Review for a marketplace item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// Review ID.
    pub id: Uuid,
    /// Item ID.
    pub item_id: Uuid,
    /// User ID.
    pub user_id: String,
    /// User display name.
    pub user_name: String,
    /// Rating value (1-5).
    pub rating: u8,
    /// Review title.
    pub title: String,
    /// Review body.
    pub body: String,
    /// Helpful count.
    pub helpful_count: u32,
    /// Is verified purchase/install.
    pub verified: bool,
    /// Item version when reviewed.
    pub version: String,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl Review {
    /// Create a new review.
    pub fn new(
        item_id: Uuid,
        user_id: &str,
        user_name: &str,
        rating: u8,
        title: &str,
        body: &str,
        version: &str,
    ) -> Self {
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            item_id,
            user_id: user_id.to_string(),
            user_name: user_name.to_string(),
            rating: rating.min(5).max(1),
            title: title.to_string(),
            body: body.to_string(),
            helpful_count: 0,
            verified: false,
            version: version.to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark as verified.
    pub fn verify(mut self) -> Self {
        self.verified = true;
        self
    }
}

/// Aggregated rating statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RatingStats {
    /// Item ID.
    pub item_id: Uuid,
    /// Average rating.
    pub average: f32,
    /// Total ratings.
    pub total: u32,
    /// Distribution by star.
    pub distribution: RatingDistribution,
}

/// Rating distribution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RatingDistribution {
    /// 1 star count.
    pub one: u32,
    /// 2 star count.
    pub two: u32,
    /// 3 star count.
    pub three: u32,
    /// 4 star count.
    pub four: u32,
    /// 5 star count.
    pub five: u32,
}

impl RatingDistribution {
    /// Add a rating.
    pub fn add(&mut self, value: u8) {
        match value {
            1 => self.one += 1,
            2 => self.two += 1,
            3 => self.three += 1,
            4 => self.four += 1,
            5 => self.five += 1,
            _ => {}
        }
    }

    /// Get total count.
    pub fn total(&self) -> u32 {
        self.one + self.two + self.three + self.four + self.five
    }

    /// Calculate average.
    pub fn average(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }

        let sum = self.one + self.two * 2 + self.three * 3 + self.four * 4 + self.five * 5;

        sum as f32 / total as f32
    }

    /// Get percentage for a star value.
    pub fn percentage(&self, value: u8) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }

        let count = match value {
            1 => self.one,
            2 => self.two,
            3 => self.three,
            4 => self.four,
            5 => self.five,
            _ => 0,
        };

        count as f32 / total as f32 * 100.0
    }
}

impl RatingStats {
    /// Create new rating stats.
    pub fn new(item_id: Uuid) -> Self {
        Self {
            item_id,
            average: 0.0,
            total: 0,
            distribution: RatingDistribution::default(),
        }
    }

    /// Add a rating.
    pub fn add_rating(&mut self, value: u8) {
        self.distribution.add(value);
        self.total = self.distribution.total();
        self.average = self.distribution.average();
    }

    /// Format as stars string.
    pub fn stars_string(&self) -> String {
        let full = self.average.floor() as usize;
        let half = (self.average - full as f32) >= 0.5;

        let mut stars = "★".repeat(full);
        if half {
            stars.push('½');
        }
        let empty = 5 - full - if half { 1 } else { 0 };
        stars.push_str(&"☆".repeat(empty));

        stars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rating() {
        let rating = Rating::new(Uuid::new_v4(), "user1", 4);
        assert_eq!(rating.value, 4);

        // Test clamping
        let high = Rating::new(Uuid::new_v4(), "user1", 10);
        assert_eq!(high.value, 5);

        let low = Rating::new(Uuid::new_v4(), "user1", 0);
        assert_eq!(low.value, 1);
    }

    #[test]
    fn test_review() {
        let review = Review::new(
            Uuid::new_v4(),
            "user1",
            "John",
            5,
            "Great plugin!",
            "This plugin is amazing.",
            "1.0.0",
        );

        assert_eq!(review.rating, 5);
        assert!(!review.verified);

        let verified = review.verify();
        assert!(verified.verified);
    }

    #[test]
    fn test_rating_stats() {
        let mut stats = RatingStats::new(Uuid::new_v4());

        stats.add_rating(5);
        stats.add_rating(5);
        stats.add_rating(4);
        stats.add_rating(3);

        assert_eq!(stats.total, 4);
        assert!((stats.average - 4.25).abs() < 0.01);
        assert_eq!(stats.distribution.five, 2);
    }

    #[test]
    fn test_stars_string() {
        let mut stats = RatingStats::new(Uuid::new_v4());
        stats.average = 3.5;

        let stars = stats.stars_string();
        assert!(stars.contains("★★★"));
    }
}
