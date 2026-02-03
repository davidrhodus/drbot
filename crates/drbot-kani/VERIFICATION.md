# Formal Verification with Kani

This crate provides formal verification proofs for critical drbot components using [Kani](https://github.com/model-checking/kani), a bit-precise model checker for Rust.

## Installation

### Prerequisites

- Rust 1.70 or later
- Kani supports Linux and macOS

### Install Kani

```bash
# Install via cargo
cargo install --locked kani-verifier
kani setup

# Or use the installer script
curl -sSf https://raw.githubusercontent.com/model-checking/kani/main/scripts/kani-install.sh | bash
```

## Running Proofs

### Run All Proofs

```bash
# Run all proofs in drbot-kani
cargo kani --package drbot-kani

# Run all proofs in a specific crate
cargo kani --package drbot-core
cargo kani --package drbot-memory
cargo kani --package drbot-state-machine
```

### Run Specific Proofs

```bash
# Run a single proof by name
cargo kani --package drbot-kani --harness proof_cosine_bounds

# Run proofs matching a pattern
cargo kani --package drbot-memory --harness proof_cosine
```

### Proof Output

Successful verification looks like:
```
VERIFICATION:- SUCCESSFUL
```

Failed verification shows a counterexample:
```
VERIFICATION:- FAILED
Counterexample:
  a = [1.0, 2.0]
  b = [3.0, 4.0]
```

## Verified Properties

### Cosine Similarity (`drbot-memory/src/longterm.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Returns 0.0 for mismatched lengths | `proof_cosine_mismatched_returns_zero` | ✓ |
| Returns 0.0 for zero vectors | `proof_cosine_zero_vector` | ✓ |
| Symmetric: `sim(a,b) == sim(b,a)` | `proof_cosine_symmetric` | ✓ |
| Identical vectors yield ~1.0 | `proof_cosine_identical_is_one` | ✓ |
| Bounds in [-1, 1] for valid inputs | `proof_cosine_bounds` | ✓ |

### Retention Score (`drbot-kani/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Non-negative for valid inputs | `proof_retention_non_negative` | ✓ |
| Zero confidence → zero score | `proof_zero_confidence_zero_retention` | ✓ |
| Critical > Low importance | `proof_importance_ordering` | ✓ |

### Confidence Clamping

| Property | Proof | Status |
|----------|-------|--------|
| Output in [0, 1] | `proof_clamp_confidence_bounds` | ✓ |
| Idempotent | `proof_clamp_idempotent` | ✓ |
| Preserves valid values | `proof_clamp_preserves_valid` | ✓ |

### Session Operations (`drbot-core/src/session.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| `last_messages` no panic | `proof_last_messages_no_panic` | ✓ |
| `last_messages(0)` is empty | `proof_last_messages_zero_is_empty` | ✓ |
| Archive changes state | `proof_archive_changes_state` | ✓ |
| Message count consistency | `proof_add_message_count` | ✓ |
| Token accumulation correct | `proof_token_usage_accumulates` | ✓ |

### State Machine (`drbot-state-machine/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Initial state correctly set | `proof_initial_state_set` | ✓ |
| Missing initial fails validation | `proof_no_initial_fails_validation` | ✓ |
| Invalid transition fails | `proof_invalid_transition_fails_validation` | ✓ |
| Valid definition passes | `proof_valid_definition_passes` | ✓ |
| `get_transition` correctness | `proof_get_transition_correct` | ✓ |

### Numeric Safety (`drbot-kani/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Saturating add no overflow | `proof_saturating_add_no_overflow` | ✓ |
| Saturating add commutative | `proof_saturating_add_commutative` | ✓ |
| Cost multiply overflow-safe | `proof_cost_multiply_overflow_safe` | ✓ |

### Retry Configuration

| Property | Proof | Status |
|----------|-------|--------|
| New config can retry | `proof_new_can_retry` | ✓ |
| Max retries exhausted | `proof_max_retries_exhausted` | ✓ |
| Attempts decrease monotonically | `proof_attempts_remaining_decreases` | ✓ |
| Current never exceeds max | `proof_current_never_exceeds_max` | ✓ |

### Quality Tier

| Property | Proof | Status |
|----------|-------|--------|
| Tier bounds [1, 5] | `proof_quality_tier_bounds` | ✓ |
| Tier 5 meets all minimums | `proof_max_tier_meets_all` | ✓ |
| `meets_minimum` reflexive | `proof_meets_minimum_reflexive` | ✓ |
| `meets_minimum` transitive | `proof_meets_minimum_transitive` | ✓ |

### Circuit Breaker (`drbot-circuit/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Default state is Closed | `proof_default_is_closed` | ✓ |
| Config has valid defaults | `proof_default_config_valid` | ✓ |
| Failure rate bounds [0, 1] | `proof_failure_rate_bounds` | ✓ |
| Empty window yields 0 rate | `proof_empty_window_zero_rate` | ✓ |
| Internal state defaults consistent | `proof_internal_state_defaults` | ✓ |
| Metrics defaults consistent | `proof_metrics_defaults` | ✓ |

### Rate Limiter (`drbot-rate-limit/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Default config valid | `proof_default_config_valid` | ✓ |
| Default algorithm is SlidingWindow | `proof_default_algorithm` | ✓ |
| Token bucket capacity bound | `proof_token_bucket_capacity_bound` | ✓ |
| Token acquire decreases tokens | `proof_token_acquire_decreases` | ✓ |
| Remaining bounded by limit | `proof_remaining_bounded_by_limit` | ✓ |
| Stats accounting consistent | `proof_stats_consistency` | ✓ |
| Refill rate calculation valid | `proof_refill_rate_calculation` | ✓ |
| Headers conversion preserves info | `proof_headers_conversion` | ✓ |

### Task Classifier (`drbot-router/src/classifier.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| TaskComplexity has 4 variants | `proof_complexity_variants` | ✓ |
| TaskType has 9 variants | `proof_task_type_variants` | ✓ |
| Length estimation deterministic | `proof_length_estimation_deterministic` | ✓ |
| Length thresholds ordered | `proof_length_thresholds_ordered` | ✓ |
| Empty messages → Simple | `proof_empty_messages_valid` | ✓ |
| Long messages → Expert | `proof_long_messages_expert` | ✓ |
| Classification thresholds valid | `proof_classification_thresholds` | ✓ |

### Consensus (`drbot-consensus/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Strategy has 7 variants | `proof_strategy_variants` | ✓ |
| Default config valid | `proof_default_config_valid` | ✓ |
| Agreement bounds [0, 1] | `proof_agreement_bounds` | ✓ |
| Weighted score non-negative | `proof_weighted_score_non_negative` | ✓ |
| Jaccard similarity bounds | `proof_jaccard_similarity_bounds` | ✓ |
| Empty sets similarity = 1.0 | `proof_empty_sets_similarity` | ✓ |
| Result consistency | `proof_result_consistency` | ✓ |

### Leader Election (`drbot-leader/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Status has 4 variants | `proof_status_variants` | ✓ |
| Lease version monotonic | `proof_lease_version_monotonic` | ✓ |
| Lease expiry logic correct | `proof_lease_expiry_logic` | ✓ |
| Default config valid | `proof_default_config_valid` | ✓ |
| Renew interval < TTL | `proof_renew_less_than_ttl` | ✓ |
| Remaining time calculation | `proof_remaining_time` | ✓ |
| Single leader invariant | `proof_single_leader_invariant` | ✓ |

### Saga Pattern (`drbot-saga/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| SagaState has 6 variants | `proof_saga_state_variants` | ✓ |
| is_finished correct | `proof_is_finished_correct` | ✓ |
| New saga is Pending | `proof_new_saga_pending` | ✓ |
| Compensation reverse order | `proof_compensation_order` | ✓ |
| Valid state transitions | `proof_valid_state_transitions` | ✓ |
| Context data roundtrip | `proof_context_roundtrip` | ✓ |
| Compensation count correct | `proof_compensation_count` | ✓ |

### Event Sourcing (`drbot-event-sourcing/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Event sequence stored correctly | `proof_event_metadata_sequence` | ✓ |
| Sequence numbers increase | `proof_sequence_increment` | ✓ |
| Version conflict detection | `proof_version_conflict_detection` | ✓ |
| Snapshot version preserved | `proof_snapshot_version_preserved` | ✓ |
| from_version filter logic | `proof_from_version_filter` | ✓ |
| Pagination logic correct | `proof_load_all_pagination` | ✓ |
| Aggregate version increment | `proof_aggregate_version_increment` | ✓ |
| Projection position monotonic | `proof_projection_position_monotonic` | ✓ |
| Snapshot frequency calculation | `proof_snapshot_frequency_calculation` | ✓ |

### Distributed Lock (`drbot-lock/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| AcquisitionStrategy has 4 variants | `proof_acquisition_strategy_variants` | ✓ |
| Token uniqueness property | `proof_token_uniqueness_property` | ✓ |
| Release requires correct token | `proof_release_token_verification` | ✓ |
| Lock expiry logic | `proof_lock_expiry_logic` | ✓ |
| Remaining time calculation | `proof_remaining_time` | ✓ |
| Extend updates expiry | `proof_extend_updates_expiry` | ✓ |
| LockStats consistency | `proof_lock_stats_consistency` | ✓ |
| Spin max attempts | `proof_spin_max_attempts` | ✓ |
| Default TTL positive | `proof_default_ttl_positive` | ✓ |
| Multi-lock rollback count | `proof_multi_lock_rollback_count` | ✓ |

### Retry Strategy (`drbot-retry-strategy/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ErrorClass has 7 variants | `proof_error_class_variants` | ✓ |
| JitterStrategy has 4 variants | `proof_jitter_strategy_variants` | ✓ |
| Default strategy valid | `proof_default_strategy_valid` | ✓ |
| Backoff no jitter deterministic | `proof_backoff_no_jitter_deterministic` | ✓ |
| Max backoff cap enforced | `proof_max_backoff_cap` | ✓ |
| Exponential growth pattern | `proof_exponential_growth` | ✓ |
| Budget token depletion | `proof_budget_token_depletion` | ✓ |
| Budget refund bounded | `proof_budget_refund_bounded` | ✓ |
| Attempt count bounds | `proof_attempt_count_bounds` | ✓ |
| is_retryable consistency | `proof_is_retryable_consistency` | ✓ |
| RetryStats consistency | `proof_retry_stats_consistency` | ✓ |
| Timeout check logic | `proof_timeout_check` | ✓ |
| Attempt number 1-indexed | `proof_attempt_number_one_indexed` | ✓ |

### Bloom Filter (`drbot-bloom/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| optimal_params returns valid values | `proof_optimal_params_valid` | ✓ |
| Bit index within bounds | `proof_bit_index_bounds` | ✓ |
| Word/offset calculation correct | `proof_word_offset_calculation` | ✓ |
| set_bit/get_bit consistency | `proof_bit_set_get_consistency` | ✓ |
| Counter saturating add | `proof_counter_saturating_add` | ✓ |
| Counter saturating sub | `proof_counter_saturating_sub` | ✓ |
| Fill ratio bounds [0, 1] | `proof_fill_ratio_bounds` | ✓ |
| num_words covers all bits | `proof_num_words_calculation` | ✓ |
| Scalable growth factor | `proof_scalable_growth` | ✓ |
| Scalable FP rate decreases | `proof_scalable_fp_decreases` | ✓ |
| No false negatives property | `proof_no_false_negatives_property` | ✓ |

### Ring Buffer (`drbot-ringbuf/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Wraparound calculation correct | `proof_wraparound_calculation` | ✓ |
| len <= capacity invariant | `proof_len_capacity_invariant` | ✓ |
| is_full logic correct | `proof_is_full_logic` | ✓ |
| is_empty logic correct | `proof_is_empty_logic` | ✓ |
| Push increases len | `proof_push_increases_len` | ✓ |
| Pop decreases len | `proof_pop_decreases_len` | ✓ |
| Available calculation correct | `proof_available_calculation` | ✓ |
| peek_back index correct | `proof_peek_back_index` | ✓ |
| push_overwrite maintains len | `proof_push_overwrite_len` | ✓ |
| Iterator remaining count | `proof_iterator_remaining` | ✓ |
| Growing buffer doubles correctly | `proof_growing_buffer_doubles` | ✓ |
| Default capacity valid | `proof_default_capacity` | ✓ |
| Head/tail within bounds | `proof_head_tail_bounds` | ✓ |

### Throttle (`drbot-throttle/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ThrottleResult has 3 variants | `proof_throttle_result_variants` | ✓ |
| Default config valid | `proof_default_config_valid` | ✓ |
| Token bucket refill logic | `proof_token_bucket_refill` | ✓ |
| Token consumption logic | `proof_token_consumption` | ✓ |
| Wait time calculation | `proof_wait_time_calculation` | ✓ |
| Sliding window count bounds | `proof_sliding_window_count` | ✓ |
| Priority multiplier logic | `proof_priority_multiplier` | ✓ |
| High priority bypass | `proof_high_priority_bypass` | ✓ |
| Semaphore permit tracking | `proof_semaphore_permit_tracking` | ✓ |
| Composite all-allowed logic | `proof_composite_all_allowed` | ✓ |
| ThrottleStats consistency | `proof_throttle_stats_consistency` | ✓ |
| Token bucket capacity respected | `proof_token_bucket_capacity` | ✓ |
| Default priority is normal | `proof_default_priority` | ✓ |

### LRU Cache (`drbot-lru/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| len <= capacity invariant | `proof_len_capacity_invariant` | ✓ |
| Eviction trigger logic | `proof_eviction_trigger` | ✓ |
| Free list reuse | `proof_free_list_reuse` | ✓ |
| Linked list prev/next consistency | `proof_linked_list_consistency` | ✓ |
| move_to_front head update | `proof_move_to_front_head_update` | ✓ |
| Unlink preserves continuity | `proof_unlink_continuity` | ✓ |
| is_empty consistency | `proof_is_empty_consistency` | ✓ |
| Insert update returns old | `proof_insert_update_returns_old` | ✓ |
| TTL expiration logic | `proof_ttl_expiration` | ✓ |
| cleanup_expired count | `proof_cleanup_expired_count` | ✓ |
| Remove decreases len | `proof_remove_decreases_len` | ✓ |
| Clear resets state | `proof_clear_resets_state` | ✓ |

### Heap (`drbot-heap/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Parent index calculation | `proof_parent_index` | ✓ |
| Left child index calculation | `proof_left_child_index` | ✓ |
| Right child index calculation | `proof_right_child_index` | ✓ |
| Min heap property | `proof_min_heap_property` | ✓ |
| Max heap property | `proof_max_heap_property` | ✓ |
| Push increases length | `proof_push_increases_len` | ✓ |
| Pop decreases length | `proof_pop_decreases_len` | ✓ |
| is_empty consistency | `proof_is_empty_consistency` | ✓ |
| sift_up terminates | `proof_sift_up_terminates` | ✓ |
| sift_down terminates | `proof_sift_down_terminates` | ✓ |
| Median heap balance | `proof_median_heap_balance` | ✓ |
| Median selection | `proof_median_selection` | ✓ |
| Priority queue ordering | `proof_priority_queue_ordering` | ✓ |
| Swap preserves elements | `proof_swap_preserves_elements` | ✓ |

### Checksum (`drbot-checksum/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| XOR is self-inverse | `proof_xor_self_inverse` | ✓ |
| XOR is commutative | `proof_xor_commutative` | ✓ |
| XOR identity (x ^ 0 = x) | `proof_xor_identity` | ✓ |
| Sum wrapping add | `proof_sum_wrapping_add` | ✓ |
| Fletcher-16 modulo bounds | `proof_fletcher16_bounds` | ✓ |
| Fletcher-16 finalize packing | `proof_fletcher16_finalize` | ✓ |
| Adler-32 modulo bounds | `proof_adler32_bounds` | ✓ |
| Adler-32 initial values | `proof_adler32_initial` | ✓ |
| Adler-32 finalize packing | `proof_adler32_finalize` | ✓ |
| CRC-32 table index bounds | `proof_crc32_table_index` | ✓ |
| CRC-32 XOR pattern | `proof_crc32_xor_pattern` | ✓ |
| Verify logic | `proof_verify_logic` | ✓ |
| Append adds 4 bytes | `proof_append_adds_4_bytes` | ✓ |
| verify_and_strip min length | `proof_verify_strip_min_length` | ✓ |
| Reset restores initial state | `proof_reset_restores_initial` | ✓ |

### Bitset (`drbot-bitset/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Word/bit index calculation | `proof_word_bit_index` | ✓ |
| num_words covers all bits | `proof_num_words_covers_all` | ✓ |
| Set operation correctness | `proof_set_operation` | ✓ |
| Clear operation correctness | `proof_clear_operation` | ✓ |
| Toggle is self-inverse | `proof_toggle_self_inverse` | ✓ |
| count_ones + count_zeros = len | `proof_count_sum` | ✓ |
| any/none consistency | `proof_any_none_consistency` | ✓ |
| all/empty mutually exclusive | `proof_all_empty_exclusive` | ✓ |
| first_set within bounds | `proof_first_set_bounds` | ✓ |
| AND operation (intersection) | `proof_bitwise_and` | ✓ |
| OR operation (union) | `proof_bitwise_or` | ✓ |
| XOR properties | `proof_bitwise_xor` | ✓ |
| NOT with mask | `proof_not_with_mask` | ✓ |
| trailing_zeros for first_set | `proof_trailing_zeros` | ✓ |
| Resize clears extra bits | `proof_resize_clears_extra` | ✓ |

### Histogram (`drbot-histogram/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Bucket width calculation | `proof_bucket_width` | ✓ |
| Bucket index within bounds | `proof_bucket_index_bounds` | ✓ |
| Count increases on record | `proof_count_increases` | ✓ |
| Sum accumulates correctly | `proof_sum_accumulates` | ✓ |
| Mean calculation | `proof_mean_calculation` | ✓ |
| Bucket range calculation | `proof_bucket_range` | ✓ |
| Percentile bounds | `proof_percentile_bounds` | ✓ |
| Cumulative percentile | `proof_cumulative_percentile` | ✓ |
| Merge preserves count sum | `proof_merge_count_sum` | ✓ |
| Reset clears state | `proof_reset_clears` | ✓ |
| Exponential bucket bounds | `proof_exp_bucket_bounds` | ✓ |
| Clamping logic | `proof_clamping` | ✓ |
| min < max validation | `proof_min_max_validation` | ✓ |

### Percentile (`drbot-percentile/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Percentile bounds validation | `proof_percentile_bounds` | ✓ |
| Quantile bounds validation | `proof_quantile_bounds` | ✓ |
| Quantile to percentile conversion | `proof_quantile_percentile_conversion` | ✓ |
| Index calculation for percentile | `proof_percentile_index` | ✓ |
| Interpolation fraction bounds | `proof_interpolation_fraction` | ✓ |
| Linear interpolation | `proof_linear_interpolation` | ✓ |
| Median is 50th percentile | `proof_median_is_p50` | ✓ |
| Quartile ordering | `proof_quartile_ordering` | ✓ |
| IQR non-negative | `proof_iqr_non_negative` | ✓ |
| Percentile rank bounds | `proof_percentile_rank_bounds` | ✓ |
| Streaming marker count | `proof_streaming_marker_count` | ✓ |
| Streaming count increases | `proof_streaming_count_increases` | ✓ |
| Desired positions formula | `proof_desired_positions` | ✓ |
| Common percentiles order | `proof_common_percentiles_order` | ✓ |

### Backpressure (`drbot-backpressure/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| LoadLevel Low < Normal | `proof_load_level_ordering_low_normal` | ✓ |
| LoadLevel Normal < High | `proof_load_level_ordering_normal_high` | ✓ |
| LoadLevel High < Critical | `proof_load_level_ordering_high_critical` | ✓ |
| LoadLevel ordering transitive | `proof_load_level_ordering_transitive` | ✓ |
| LoadLevel factor bounds [0, 1] | `proof_load_level_factor_bounds` | ✓ |
| LoadLevel factor monotonic | `proof_load_level_factor_monotonic` | ✓ |
| Priority Low < Normal | `proof_priority_ordering_low_normal` | ✓ |
| Priority Normal < High | `proof_priority_ordering_normal_high` | ✓ |
| Priority High < Critical | `proof_priority_ordering_high_critical` | ✓ |
| Priority ordering transitive | `proof_priority_ordering_transitive` | ✓ |
| Priority default is Normal | `proof_priority_default` | ✓ |
| Priority numeric values | `proof_priority_values` | ✓ |
| LoadThresholds default valid | `proof_load_thresholds_default_valid` | ✓ |
| LoadThresholds default ascending | `proof_load_thresholds_default_ascending` | ✓ |
| Shed ratio zero when empty | `proof_shed_ratio_zero_when_empty` | ✓ |
| Shed ratio bounds [0, 1] | `proof_shed_ratio_bounds` | ✓ |
| Shed ratio = 1 when all shed | `proof_shed_ratio_all_shed` | ✓ |
| Shed ratio = 0 when none shed | `proof_shed_ratio_none_shed` | ✓ |
| Queue len_estimate initial | `proof_queue_len_estimate_initial` | ✓ |
| Queue len_estimate correct | `proof_queue_len_estimate_correct` | ✓ |
| Queue len_estimate saturating | `proof_queue_len_estimate_saturating` | ✓ |
| Queue is_empty logic | `proof_queue_is_empty` | ✓ |
| SignalType variants distinct | `proof_signal_type_variants` | ✓ |

### ID Generation (`drbot-id/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ULID timestamp extraction (small) | `proof_ulid_timestamp_extraction_low` | ✓ |
| ULID timestamp extraction (48-bit) | `proof_ulid_timestamp_extraction_48bit` | ✓ |
| ULID timestamp byte boundaries | `proof_ulid_timestamp_byte_boundaries` | ✓ |
| ULID encoding table size | `proof_ulid_encoding_table_size` | ✓ |
| ULID bytes length | `proof_ulid_bytes_length` | ✓ |
| Snowflake machine ID masking | `proof_snowflake_machine_id_masking` | ✓ |
| Snowflake sequence masking | `proof_snowflake_sequence_masking` | ✓ |
| Snowflake timestamp masking | `proof_snowflake_timestamp_masking` | ✓ |
| Snowflake ID composition | `proof_snowflake_id_composition` | ✓ |
| Snowflake extract machine ID | `proof_snowflake_extract_machine_id` | ✓ |
| Snowflake extract sequence | `proof_snowflake_extract_sequence` | ✓ |
| Snowflake bit layout no overlap | `proof_snowflake_bit_layout_no_overlap` | ✓ |
| ShortId default alphabet size | `proof_short_id_default_alphabet_size` | ✓ |
| ShortId default length | `proof_short_id_default_length` | ✓ |
| PrefixedId parse none without underscore | `proof_prefixed_id_parse_none_without_underscore` | ✓ |
| PrefixedId parse with underscore | `proof_prefixed_id_parse_with_underscore` | ✓ |
| PrefixedId has_prefix logic | `proof_prefixed_id_has_prefix` | ✓ |

### Idempotency (`drbot-idempotency/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| IdempotencyStatus variants distinct | `proof_idempotency_status_variants` | ✓ |
| IdempotencyStatus Processing is initial | `proof_idempotency_status_processing_initial` | ✓ |
| AcquireResult Acquired distinct | `proof_acquire_result_acquired_distinct` | ✓ |
| AcquireResult no cross-match | `proof_acquire_result_not_cross_match` | ✓ |
| IdempotencyResult Processed has value | `proof_idempotency_result_processed_has_value` | ✓ |
| IdempotencyResult Cached has value | `proof_idempotency_result_cached_has_value` | ✓ |
| IdempotencyResult InProgress no value | `proof_idempotency_result_in_progress_no_value` | ✓ |
| IdempotencyResult Conflict no value | `proof_idempotency_result_conflict_no_value` | ✓ |
| IdempotencyResult into_value Processed | `proof_idempotency_result_into_value_processed` | ✓ |
| IdempotencyResult into_value Cached | `proof_idempotency_result_into_value_cached` | ✓ |
| IdempotencyResult into_value InProgress | `proof_idempotency_result_into_value_in_progress` | ✓ |
| IdempotencyResult into_value Conflict | `proof_idempotency_result_into_value_conflict` | ✓ |
| Fingerprint deterministic | `proof_fingerprint_deterministic` | ✓ |
| Fingerprint different methods | `proof_fingerprint_different_methods` | ✓ |
| Fingerprint different paths | `proof_fingerprint_different_paths` | ✓ |
| Record initial status | `proof_record_initial_status` | ✓ |
| Lock expired when none | `proof_lock_expired_when_none` | ✓ |
| Config default TTL positive | `proof_config_default_ttl_positive` | ✓ |
| Config default lock TTL positive | `proof_config_default_lock_ttl_positive` | ✓ |
| Config lock TTL < record TTL | `proof_config_lock_ttl_less_than_ttl` | ✓ |

### Deque (`drbot-deque/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| BoundedDeque len <= capacity | `proof_bounded_deque_len_capacity_invariant` | ✓ |
| BoundedDeque is_full logic | `proof_bounded_deque_is_full_logic` | ✓ |
| BoundedDeque is_empty logic | `proof_bounded_deque_is_empty_logic` | ✓ |
| BoundedDeque push_back increases len | `proof_bounded_deque_push_back_increases_len` | ✓ |
| BoundedDeque push_front increases len | `proof_bounded_deque_push_front_increases_len` | ✓ |
| BoundedDeque pop_front decreases len | `proof_bounded_deque_pop_front_decreases_len` | ✓ |
| BoundedDeque pop_back decreases len | `proof_bounded_deque_pop_back_decreases_len` | ✓ |
| BoundedDeque full rejects push | `proof_bounded_deque_full_rejects_push` | ✓ |
| BoundedDeque default capacity | `proof_bounded_deque_default_capacity` | ✓ |
| SlidingWindow len <= size | `proof_sliding_window_len_size_invariant` | ✓ |
| SlidingWindow is_full logic | `proof_sliding_window_is_full_logic` | ✓ |
| SlidingWindow push when full removes oldest | `proof_sliding_window_push_when_full_removes_oldest` | ✓ |
| SlidingWindow push not full returns none | `proof_sliding_window_push_when_not_full_returns_none` | ✓ |
| SlidingWindow default size | `proof_sliding_window_default_size` | ✓ |
| PriorityDeque empty initially | `proof_priority_deque_empty_initially` | ✓ |
| PriorityDeque push increases len | `proof_priority_deque_push_increases_len` | ✓ |
| PriorityDeque pop decreases len | `proof_priority_deque_pop_decreases_len` | ✓ |
| PriorityDeque ordering | `proof_priority_deque_ordering` | ✓ |
| PriorityDeque with capacity full | `proof_priority_deque_with_capacity_full` | ✓ |
| WorkStealingDeque empty initially | `proof_work_stealing_deque_empty_initially` | ✓ |
| WorkStealingDeque LIFO local | `proof_work_stealing_deque_lifo_local` | ✓ |
| WorkStealingDeque FIFO steal | `proof_work_stealing_deque_fifo_steal` | ✓ |
| SharedDeque empty initially | `proof_shared_deque_empty_initially` | ✓ |
| SharedDeque push/pop front | `proof_shared_deque_push_pop_front` | ✓ |
| SharedDeque push/pop back | `proof_shared_deque_push_pop_back` | ✓ |

### Priority Queue (`drbot-pqueue/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Parent index calculation | `proof_parent_index_calculation` | ✓ |
| Left child index calculation | `proof_left_child_index_calculation` | ✓ |
| Right child index calculation | `proof_right_child_index_calculation` | ✓ |
| Children adjacent | `proof_children_adjacent` | ✓ |
| PriorityQueue empty initially | `proof_priority_queue_empty_initially` | ✓ |
| PriorityQueue push increases len | `proof_priority_queue_push_increases_len` | ✓ |
| PriorityQueue pop decreases len | `proof_priority_queue_pop_decreases_len` | ✓ |
| PriorityQueue pop empty | `proof_priority_queue_pop_empty` | ✓ |
| PriorityQueue peek no remove | `proof_priority_queue_peek_does_not_remove` | ✓ |
| PriorityQueue max-heap property | `proof_priority_queue_max_heap_property` | ✓ |
| PriorityQueue with capacity full | `proof_priority_queue_with_capacity_full` | ✓ |
| PriorityQueue unbounded not full | `proof_priority_queue_unbounded_not_full` | ✓ |
| PriorityQueue clear | `proof_priority_queue_clear` | ✓ |
| MinPriorityQueue empty initially | `proof_min_priority_queue_empty_initially` | ✓ |
| MinPriorityQueue min first | `proof_min_priority_queue_min_first` | ✓ |
| MinPriorityQueue push increases len | `proof_min_priority_queue_push_increases_len` | ✓ |
| KeyedPQ empty initially | `proof_keyed_pq_empty_initially` | ✓ |
| KeyedPQ contains key after push | `proof_keyed_pq_contains_key_after_push` | ✓ |
| KeyedPQ update replaces value | `proof_keyed_pq_update_replaces_value` | ✓ |
| KeyedPQ remove by key | `proof_keyed_pq_remove_by_key` | ✓ |
| KeyedPQ remove nonexistent | `proof_keyed_pq_remove_nonexistent` | ✓ |
| SyncPQ empty initially | `proof_sync_pq_empty_initially` | ✓ |
| SyncPQ push pop | `proof_sync_pq_push_pop` | ✓ |

### Semantic Versioning (`drbot-semver/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Version equality reflexive | `proof_version_equality_reflexive` | ✓ |
| Version comparison major precedence | `proof_version_comparison_major_takes_precedence` | ✓ |
| Version comparison minor second | `proof_version_comparison_minor_second` | ✓ |
| Version comparison patch third | `proof_version_comparison_patch_third` | ✓ |
| Release > prerelease | `proof_version_release_greater_than_prerelease` | ✓ |
| Version comparison symmetric | `proof_version_comparison_symmetric` | ✓ |
| increment_major resets minor/patch | `proof_increment_major_resets_minor_patch` | ✓ |
| increment_minor resets patch | `proof_increment_minor_resets_patch` | ✓ |
| increment_patch only | `proof_increment_patch_only` | ✓ |
| Increment produces greater version | `proof_increment_produces_greater_version` | ✓ |
| is_stable requires major >= 1 | `proof_is_stable_requires_major_ge_1` | ✓ |
| is_stable requires no prerelease | `proof_is_stable_requires_no_prerelease` | ✓ |
| is_prerelease logic | `proof_is_prerelease_logic` | ✓ |
| Compatibility same major post-1 | `proof_compatibility_same_major_post_1` | ✓ |
| Compatibility different major post-1 | `proof_compatibility_different_major_post_1` | ✓ |
| Compatibility 0.x requires same minor | `proof_compatibility_0_x_requires_same_minor` | ✓ |
| Operator variants distinct | `proof_operator_variants` | ✓ |
| Constraint exact match | `proof_constraint_exact_match` | ✓ |
| Constraint greater | `proof_constraint_greater` | ✓ |
| Constraint greater_eq | `proof_constraint_greater_eq` | ✓ |
| Constraint less | `proof_constraint_less` | ✓ |
| Constraint less_eq | `proof_constraint_less_eq` | ✓ |
| Constraint tilde | `proof_constraint_tilde` | ✓ |
| Constraint caret stable | `proof_constraint_caret_stable` | ✓ |
| VersionRange empty matches all | `proof_version_range_empty_matches_all` | ✓ |
| VersionRange single constraint | `proof_version_range_single_constraint` | ✓ |
| VersionRange multiple constraints | `proof_version_range_multiple_constraints` | ✓ |

### Interval (`drbot-interval/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Bound Inclusive has value | `proof_bound_inclusive_has_value` | ✓ |
| Bound Exclusive has value | `proof_bound_exclusive_has_value` | ✓ |
| Bound Unbounded no value | `proof_bound_unbounded_no_value` | ✓ |
| Closed interval contains endpoints | `proof_closed_interval_contains_endpoints` | ✓ |
| Open interval excludes endpoints | `proof_open_interval_excludes_endpoints` | ✓ |
| Half-open interval bounds | `proof_half_open_interval_bounds` | ✓ |
| Unbounded interval contains all | `proof_unbounded_interval_contains_all` | ✓ |
| Closed interval contains middle | `proof_closed_interval_contains_middle` | ✓ |
| Closed interval excludes outside | `proof_closed_interval_excludes_outside` | ✓ |
| Closed interval invalid when start > end | `proof_closed_interval_invalid_when_start_greater_than_end` | ✓ |
| Open interval invalid when start >= end | `proof_open_interval_invalid_when_start_ge_end` | ✓ |
| IntRange len correct | `proof_int_range_len_correct` | ✓ |
| IntRange is_empty logic | `proof_int_range_is_empty_logic` | ✓ |
| IntRange contains logic | `proof_int_range_contains_logic` | ✓ |
| IntRange single contains only value | `proof_int_range_single_contains_only_value` | ✓ |
| IntRange overlap symmetric | `proof_int_range_overlap_symmetric` | ✓ |
| IntRange no overlap disjoint | `proof_int_range_no_overlap_disjoint` | ✓ |
| IntRange intersection within both | `proof_int_range_intersection_within_both` | ✓ |
| IntRange union contiguous | `proof_int_range_union_contiguous` | ✓ |
| IntervalSet empty initially | `proof_interval_set_empty_initially` | ✓ |
| IntervalSet insert increases len | `proof_interval_set_insert_increases_len` | ✓ |
| IntervalSet contains after insert | `proof_interval_set_contains_after_insert` | ✓ |

### Trie (`drbot-trie/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Trie empty initially | `proof_trie_empty_initially` | ✓ |
| Trie insert increases len | `proof_trie_insert_increases_len` | ✓ |
| Trie insert same key returns old | `proof_trie_insert_same_key_returns_old` | ✓ |
| Trie get after insert | `proof_trie_get_after_insert` | ✓ |
| Trie get nonexistent | `proof_trie_get_nonexistent` | ✓ |
| Trie contains after insert | `proof_trie_contains_after_insert` | ✓ |
| Trie contains prefix but not key | `proof_trie_contains_prefix_but_not_key` | ✓ |
| Trie has_prefix after insert | `proof_trie_has_prefix_after_insert` | ✓ |
| Trie remove decreases len | `proof_trie_remove_decreases_len` | ✓ |
| Trie remove nonexistent | `proof_trie_remove_nonexistent` | ✓ |
| Trie remove preserves other keys | `proof_trie_remove_preserves_other_keys` | ✓ |
| Trie clear resets | `proof_trie_clear_resets` | ✓ |
| Trie empty key | `proof_trie_empty_key` | ✓ |
| StringTrie empty initially | `proof_string_trie_empty_initially` | ✓ |
| StringTrie insert contains | `proof_string_trie_insert_contains` | ✓ |
| StringTrie remove | `proof_string_trie_remove` | ✓ |
| CountingTrie empty initially | `proof_counting_trie_empty_initially` | ✓ |
| CountingTrie add increments | `proof_counting_trie_add_increments` | ✓ |
| CountingTrie len unique words | `proof_counting_trie_len_unique_words` | ✓ |

### BiMap (`drbot-bimap/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| BiMap empty initially | `proof_bimap_empty_initially` | ✓ |
| BiMap insert increases len | `proof_bimap_insert_increases_len` | ✓ |
| BiMap bidirectional lookup | `proof_bimap_bidirectional_lookup` | ✓ |
| BiMap consistency invariant | `proof_bimap_consistency_invariant` | ✓ |
| BiMap contains key and value | `proof_bimap_contains_key_and_value` | ✓ |
| BiMap insert overwrite key | `proof_bimap_insert_overwrite_key` | ✓ |
| BiMap insert overwrite value | `proof_bimap_insert_overwrite_value` | ✓ |
| BiMap insert no overwrite success | `proof_bimap_insert_no_overwrite_success` | ✓ |
| BiMap insert no overwrite key exists | `proof_bimap_insert_no_overwrite_key_exists` | ✓ |
| BiMap insert no overwrite value exists | `proof_bimap_insert_no_overwrite_value_exists` | ✓ |
| BiMap remove by key | `proof_bimap_remove_by_key` | ✓ |
| BiMap remove by value | `proof_bimap_remove_by_value` | ✓ |
| BiMap remove nonexistent | `proof_bimap_remove_nonexistent` | ✓ |
| BiMap remove preserves others | `proof_bimap_remove_preserves_others` | ✓ |
| BiMap clear | `proof_bimap_clear` | ✓ |
| BiMap multiple inserts | `proof_bimap_multiple_inserts` | ✓ |
| BiMap len consistency | `proof_bimap_len_consistency` | ✓ |

### DAG (`drbot-dag/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| DAG empty initially | `proof_dag_empty_initially` | ✓ |
| Add node increases count | `proof_dag_add_node_increases_count` | ✓ |
| Add duplicate node fails | `proof_dag_add_duplicate_node_fails` | ✓ |
| Add edge requires nodes | `proof_dag_add_edge_requires_nodes` | ✓ |
| Add edge success | `proof_dag_add_edge_success` | ✓ |
| Edge bidirectional tracking | `proof_dag_edge_bidirectional_tracking` | ✓ |
| Self-loop prevented | `proof_dag_self_loop_prevented` | ✓ |
| Direct cycle prevented | `proof_dag_direct_cycle_prevented` | ✓ |
| Indirect cycle prevented | `proof_dag_indirect_cycle_prevented` | ✓ |
| Remove node decreases count | `proof_dag_remove_node_decreases_count` | ✓ |
| Remove node removes edges | `proof_dag_remove_node_removes_edges` | ✓ |
| Remove edge | `proof_dag_remove_edge` | ✓ |
| Remove nonexistent | `proof_dag_remove_nonexistent` | ✓ |
| Single node is root and leaf | `proof_dag_single_node_is_root_and_leaf` | ✓ |
| Roots have no predecessors | `proof_dag_roots_have_no_predecessors` | ✓ |
| Leaves have no successors | `proof_dag_leaves_have_no_successors` | ✓ |
| Clear | `proof_dag_clear` | ✓ |
| Node count consistency | `proof_dag_node_count_consistency` | ✓ |
| Edge count consistency | `proof_dag_edge_count_consistency` | ✓ |

### Stack (`drbot-stack/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Stack empty initially | `proof_stack_empty_initially` | ✓ |
| Stack push increases len | `proof_stack_push_increases_len` | ✓ |
| Stack pop decreases len | `proof_stack_pop_decreases_len` | ✓ |
| Stack LIFO order | `proof_stack_lifo_order` | ✓ |
| Stack peek does not remove | `proof_stack_peek_does_not_remove` | ✓ |
| Stack bounded full | `proof_stack_bounded_full` | ✓ |
| Stack unbounded not full | `proof_stack_unbounded_not_full` | ✓ |
| Stack clear | `proof_stack_clear` | ✓ |
| MinStack empty initially | `proof_min_stack_empty_initially` | ✓ |
| MinStack tracks minimum | `proof_min_stack_tracks_minimum` | ✓ |
| MinStack min after pop | `proof_min_stack_min_after_pop` | ✓ |
| MinStack min is always minimum | `proof_min_stack_min_is_always_minimum` | ✓ |
| MaxStack empty initially | `proof_max_stack_empty_initially` | ✓ |
| MaxStack tracks maximum | `proof_max_stack_tracks_maximum` | ✓ |
| MaxStack max after pop | `proof_max_stack_max_after_pop` | ✓ |
| UndoStack empty initially | `proof_undo_stack_empty_initially` | ✓ |
| UndoStack push enables undo | `proof_undo_stack_push_enables_undo` | ✓ |
| UndoStack push clears redo | `proof_undo_stack_push_clears_redo` | ✓ |
| UndoStack current | `proof_undo_stack_current` | ✓ |
| UndoStack undo returns state | `proof_undo_stack_undo_returns_state` | ✓ |
| UndoStack max size | `proof_undo_stack_max_size` | ✓ |
| SyncStack empty initially | `proof_sync_stack_empty_initially` | ✓ |
| SyncStack push pop | `proof_sync_stack_push_pop` | ✓ |

### MultiMap (`drbot-multimap/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| MultiMap new is empty | `proof_multimap_new_is_empty` | ✓ |
| MultiMap insert creates key | `proof_multimap_insert_creates_key` | ✓ |
| MultiMap insert increases total | `proof_multimap_insert_increases_total` | ✓ |
| MultiMap allows duplicates | `proof_multimap_allows_duplicates` | ✓ |
| MultiMap get after insert | `proof_multimap_get_after_insert` | ✓ |
| MultiMap get first/last | `proof_multimap_get_first_last` | ✓ |
| MultiMap remove returns all | `proof_multimap_remove_returns_all` | ✓ |
| MultiMap remove value decreases count | `proof_multimap_remove_value_decreases_count` | ✓ |
| MultiMap remove last value removes key | `proof_multimap_remove_last_value_removes_key` | ✓ |
| MultiMap remove nonexistent value | `proof_multimap_remove_nonexistent_value` | ✓ |
| MultiMap clear | `proof_multimap_clear` | ✓ |
| MultiMap is_empty consistency | `proof_multimap_is_empty_consistency` | ✓ |
| MultiMap multiple keys | `proof_multimap_multiple_keys` | ✓ |
| MultiMapSet new is empty | `proof_multimap_set_new_is_empty` | ✓ |
| MultiMapSet insert creates key | `proof_multimap_set_insert_creates_key` | ✓ |
| MultiMapSet rejects duplicates | `proof_multimap_set_rejects_duplicates` | ✓ |
| MultiMapSet allows different values | `proof_multimap_set_allows_different_values` | ✓ |
| MultiMapSet contains | `proof_multimap_set_contains` | ✓ |
| MultiMapSet remove value | `proof_multimap_set_remove_value` | ✓ |
| MultiMapSet remove last value removes key | `proof_multimap_set_remove_last_value_removes_key` | ✓ |
| MultiMapSet remove all | `proof_multimap_set_remove_all` | ✓ |
| MultiMapSet clear | `proof_multimap_set_clear` | ✓ |
| MultiMapSet is_empty consistency | `proof_multimap_set_is_empty_consistency` | ✓ |
| MultiMapSet contains key after insert | `proof_multimap_set_contains_key_after_insert` | ✓ |
| MultiMapSet get consistency | `proof_multimap_set_get_consistency` | ✓ |

### AVL Tree (`drbot-avl/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| AVL empty initially | `proof_avl_empty_initially` | ✓ |
| AVL insert increases len | `proof_avl_insert_increases_len` | ✓ |
| AVL get after insert | `proof_avl_get_after_insert` | ✓ |
| AVL contains after insert | `proof_avl_contains_after_insert` | ✓ |
| AVL update overwrites | `proof_avl_update_overwrites` | ✓ |
| AVL balanced after insert | `proof_avl_balanced_after_insert` | ✓ |
| AVL height positive when nonempty | `proof_avl_height_positive_when_nonempty` | ✓ |
| AVL min/max empty | `proof_avl_min_max_empty` | ✓ |
| AVL min/max single | `proof_avl_min_max_single` | ✓ |
| AVL min is smallest | `proof_avl_min_is_smallest` | ✓ |
| AVL max is largest | `proof_avl_max_is_largest` | ✓ |
| AVL clear | `proof_avl_clear` | ✓ |
| AVL node height None | `proof_avl_node_height_none` | ✓ |
| AVL node new height is one | `proof_avl_node_new_height_is_one` | ✓ |
| AVL balance factor bounds | `proof_avl_balance_factor_bounds` | ✓ |
| AVL inorder sorted two | `proof_avl_inorder_sorted_two` | ✓ |
| AVL inorder sorted three | `proof_avl_inorder_sorted_three` | ✓ |

### Bounded Types (`drbot-bounded/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Bounded valid bounds | `proof_bounded_valid_bounds` | ✓ |
| Bounded below min fails | `proof_bounded_below_min_fails` | ✓ |
| Bounded above max fails | `proof_bounded_above_max_fails` | ✓ |
| Bounded invalid bounds fails | `proof_bounded_invalid_bounds_fails` | ✓ |
| Bounded clamped within bounds | `proof_bounded_clamped_within_bounds` | ✓ |
| Bounded clamped preserves valid | `proof_bounded_clamped_preserves_valid` | ✓ |
| Bounded is_at_min | `proof_bounded_is_at_min` | ✓ |
| Bounded is_at_max | `proof_bounded_is_at_max` | ✓ |
| Bounded set valid | `proof_bounded_set_valid` | ✓ |
| Bounded set clamped | `proof_bounded_set_clamped` | ✓ |
| Percentage valid | `proof_percentage_valid` | ✓ |
| Percentage above 100 fails | `proof_percentage_above_100_fails` | ✓ |
| Percentage clamped bounds | `proof_percentage_clamped_bounds` | ✓ |
| Percentage as_ratio bounds | `proof_percentage_as_ratio_bounds` | ✓ |
| Percentage constants | `proof_percentage_constants` | ✓ |
| UnitInterval below zero fails | `proof_unit_interval_below_zero_fails` | ✓ |
| UnitInterval above one fails | `proof_unit_interval_above_one_fails` | ✓ |
| UnitInterval constants | `proof_unit_interval_constants` | ✓ |
| UnitInterval complement | `proof_unit_interval_complement` | ✓ |
| UnitInterval lerp endpoints | `proof_unit_interval_lerp_endpoints` | ✓ |
| Degrees normalize positive | `proof_degrees_normalize_positive` | ✓ |
| Degrees bounds | `proof_degrees_bounds` | ✓ |
| Degrees add | `proof_degrees_add` | ✓ |
| Degrees add wrap | `proof_degrees_add_wrap` | ✓ |
| ByteValue new | `proof_byte_value_new` | ✓ |
| ByteValue min/max | `proof_byte_value_min_max` | ✓ |
| ByteValue from_float bounds | `proof_byte_value_from_float_bounds` | ✓ |
| ByteValue from_float clamps | `proof_byte_value_from_float_clamps` | ✓ |
| ByteValue to_float bounds | `proof_byte_value_to_float_bounds` | ✓ |

### Vec Extensions (`drbot-vec-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| VecExt get_or_err valid | `proof_vec_ext_get_or_err_valid` | ✓ |
| VecExt get_or_err invalid | `proof_vec_ext_get_or_err_invalid` | ✓ |
| VecExt pop_or_err nonempty | `proof_vec_ext_pop_or_err_nonempty` | ✓ |
| VecExt pop_or_err empty | `proof_vec_ext_pop_or_err_empty` | ✓ |
| VecExt first_or_err nonempty | `proof_vec_ext_first_or_err_nonempty` | ✓ |
| VecExt first_or_err empty | `proof_vec_ext_first_or_err_empty` | ✓ |
| VecExt last_or_err nonempty | `proof_vec_ext_last_or_err_nonempty` | ✓ |
| VecExt last_or_err empty | `proof_vec_ext_last_or_err_empty` | ✓ |
| VecExt remove_at valid | `proof_vec_ext_remove_at_valid` | ✓ |
| VecExt remove_at invalid | `proof_vec_ext_remove_at_invalid` | ✓ |
| VecExt insert_at valid | `proof_vec_ext_insert_at_valid` | ✓ |
| VecExt insert_at end | `proof_vec_ext_insert_at_end` | ✓ |
| VecExt insert_at invalid | `proof_vec_ext_insert_at_invalid` | ✓ |
| VecExt swap_remove_at valid | `proof_vec_ext_swap_remove_at_valid` | ✓ |
| VecExt swap_remove_at invalid | `proof_vec_ext_swap_remove_at_invalid` | ✓ |
| singleton | `proof_singleton` | ✓ |
| repeat length | `proof_repeat_length` | ✓ |
| repeat values | `proof_repeat_values` | ✓ |
| truncate | `proof_truncate` | ✓ |
| zip length | `proof_zip_length` | ✓ |
| interleave length | `proof_interleave_length` | ✓ |
| interleave order | `proof_interleave_order` | ✓ |
| VecBuilder new empty | `proof_vec_builder_new_empty` | ✓ |
| VecBuilder push | `proof_vec_builder_push` | ✓ |
| VecBuilder push_all | `proof_vec_builder_push_all` | ✓ |

### Result Extensions (`drbot-result-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ignore_err on Ok | `proof_ignore_err_ok` | ✓ |
| ignore_err on Err | `proof_ignore_err_err` | ✓ |
| replace on Ok | `proof_replace_ok` | ✓ |
| replace on Err | `proof_replace_err` | ✓ |
| replace_with on Ok | `proof_replace_with_ok` | ✓ |
| unwrap_or_default_with on Ok | `proof_unwrap_or_default_with_ok` | ✓ |
| unwrap_or_default_with on Err | `proof_unwrap_or_default_with_err` | ✓ |
| transpose_inner Some | `proof_transpose_inner_some` | ✓ |
| transpose_inner None | `proof_transpose_inner_none` | ✓ |
| transpose_inner Err | `proof_transpose_inner_err` | ✓ |
| unwrap_inner_or Some | `proof_unwrap_inner_or_some` | ✓ |
| unwrap_inner_or None | `proof_unwrap_inner_or_none` | ✓ |
| unwrap_inner_or Err | `proof_unwrap_inner_or_err` | ✓ |
| partition empty | `proof_partition_empty` | ✓ |
| partition all Ok | `proof_partition_all_ok` | ✓ |
| partition all Err | `proof_partition_all_err` | ✓ |
| partition mixed | `proof_partition_mixed` | ✓ |
| first_ok all Err | `proof_first_ok_all_err` | ✓ |
| first_ok finds first | `proof_first_ok_finds_first` | ✓ |
| ok function | `proof_ok_function` | ✓ |
| err function | `proof_err_function` | ✓ |
| combine all Ok | `proof_combine_all_ok` | ✓ |
| combine with Err | `proof_combine_with_err` | ✓ |
| any_ok finds Ok | `proof_any_ok_finds_ok` | ✓ |
| any_ok all Err | `proof_any_ok_all_err` | ✓ |

### Array Utilities (`drbot-array-util/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| from_fn length | `proof_from_fn_length` | ✓ |
| from_fn values | `proof_from_fn_values` | ✓ |
| filled all same | `proof_filled_all_same` | ✓ |
| default_array | `proof_default_array` | ✓ |
| try_from_slice valid | `proof_try_from_slice_valid` | ✓ |
| try_from_slice wrong size | `proof_try_from_slice_wrong_size` | ✓ |
| map preserves length | `proof_map_preserves_length` | ✓ |
| map applies function | `proof_map_applies_function` | ✓ |
| zip pairs correctly | `proof_zip_pairs_correctly` | ✓ |
| unzip reverses zip | `proof_unzip_reverses_zip` | ✓ |
| reverse reverses | `proof_reverse_reverses` | ✓ |
| reverse double is identity | `proof_reverse_double_is_identity` | ✓ |
| rotate_left zero | `proof_rotate_left_zero` | ✓ |
| rotate_left one | `proof_rotate_left_one` | ✓ |
| rotate_right one | `proof_rotate_right_one` | ✓ |
| rotate full cycle | `proof_rotate_full_cycle` | ✓ |
| len correct | `proof_len_correct` | ✓ |
| is_empty nonempty | `proof_is_empty_nonempty` | ✓ |
| is_empty empty | `proof_is_empty_empty` | ✓ |
| fold sum | `proof_fold_sum` | ✓ |
| all true | `proof_all_true` | ✓ |
| all false | `proof_all_false` | ✓ |
| any true | `proof_any_true` | ✓ |
| any false | `proof_any_false` | ✓ |
| transpose dimensions | `proof_transpose_dimensions` | ✓ |
| transpose values | `proof_transpose_values` | ✓ |
| transpose double is identity | `proof_transpose_double_is_identity` | ✓ |
| map 2d | `proof_map_2d` | ✓ |

### Either Type (`drbot-either/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Either Left is_left | `proof_either_left_is_left` | ✓ |
| Either Right is_right | `proof_either_right_is_right` | ✓ |
| Either Left extraction | `proof_either_left_extraction` | ✓ |
| Either Left no right | `proof_either_left_no_right` | ✓ |
| Either Right extraction | `proof_either_right_extraction` | ✓ |
| Either Right no left | `proof_either_right_no_left` | ✓ |
| Either map_left on Left | `proof_either_map_left_on_left` | ✓ |
| Either map_left on Right | `proof_either_map_left_on_right` | ✓ |
| Either map_right on Right | `proof_either_map_right_on_right` | ✓ |
| Either map_right on Left | `proof_either_map_right_on_left` | ✓ |
| Either flip Left | `proof_either_flip_left` | ✓ |
| Either flip Right | `proof_either_flip_right` | ✓ |
| Either flip double | `proof_either_flip_double` | ✓ |
| Either left_or on Left | `proof_either_left_or_on_left` | ✓ |
| Either left_or on Right | `proof_either_left_or_on_right` | ✓ |
| Either right_or on Right | `proof_either_right_or_on_right` | ✓ |
| Either right_or on Left | `proof_either_right_or_on_left` | ✓ |
| Either into_inner Left | `proof_either_into_inner_left` | ✓ |
| Either into_inner Right | `proof_either_into_inner_right` | ✓ |
| Either3 First is_first | `proof_either3_first_is_first` | ✓ |
| Either3 Second is_second | `proof_either3_second_is_second` | ✓ |
| Either3 Third is_third | `proof_either3_third_is_third` | ✓ |
| Either3 variant_index | `proof_either3_variant_index` | ✓ |
| Either3 extraction | `proof_either3_extraction` | ✓ |
| Either3 into_inner | `proof_either3_into_inner` | ✓ |
| left function | `proof_left_function` | ✓ |
| right function | `proof_right_function` | ✓ |

### Triple Types (`drbot-triple/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Triple new stores values | `proof_triple_new_stores_values` | ✓ |
| Triple into_tuple preserves | `proof_triple_into_tuple_preserves_values` | ✓ |
| Triple from tuple roundtrip | `proof_triple_from_tuple_roundtrip` | ✓ |
| Triple map_first | `proof_triple_map_first` | ✓ |
| Triple map_second | `proof_triple_map_second` | ✓ |
| Triple map_third | `proof_triple_map_third` | ✓ |
| Triple first_pair | `proof_triple_first_pair` | ✓ |
| Triple last_pair | `proof_triple_last_pair` | ✓ |
| Triple3 new stores values | `proof_triple3_new_stores_values` | ✓ |
| Triple3 accessors | `proof_triple3_accessors` | ✓ |
| Triple3 get valid indices | `proof_triple3_get_valid_indices` | ✓ |
| Triple3 get invalid index | `proof_triple3_get_invalid_index` | ✓ |
| Triple3 into_array | `proof_triple3_into_array` | ✓ |
| Triple3 from array | `proof_triple3_from_array` | ✓ |
| Triple3 sum | `proof_triple3_sum` | ✓ |
| Triple3 product | `proof_triple3_product` | ✓ |
| Triple3 min | `proof_triple3_min` | ✓ |
| Triple3 max | `proof_triple3_max` | ✓ |
| Triple3 fold | `proof_triple3_fold` | ✓ |
| Rgb new stores values | `proof_rgb_new_stores_values` | ✓ |
| Rgb into_array | `proof_rgb_into_array` | ✓ |
| Rgb from_hex to_hex roundtrip | `proof_rgb_from_hex_to_hex_roundtrip` | ✓ |
| Rgb from_hex components | `proof_rgb_from_hex_components` | ✓ |
| Rgb map | `proof_rgb_map` | ✓ |
| Xyz new stores values | `proof_xyz_new_stores_values` | ✓ |
| Xyz into_array | `proof_xyz_into_array` | ✓ |
| Xyz map | `proof_xyz_map` | ✓ |
| Xyz magnitude squared non-negative | `proof_xyz_magnitude_squared_non_negative` | ✓ |
| Xyz dot product commutative | `proof_xyz_dot_product_commutative` | ✓ |
| Xyz dot with self equals magnitude squared | `proof_xyz_dot_with_self_equals_magnitude_squared` | ✓ |

### Clamp Utilities (`drbot-clamp/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| clamp result in range | `proof_clamp_result_in_range` | ✓ |
| clamp preserves value in range | `proof_clamp_preserves_value_in_range` | ✓ |
| clamp below min returns min | `proof_clamp_below_min_returns_min` | ✓ |
| clamp above max returns max | `proof_clamp_above_max_returns_max` | ✓ |
| clamp idempotent | `proof_clamp_idempotent` | ✓ |
| clamp_min result >= min | `proof_clamp_min_result_ge_min` | ✓ |
| clamp_min preserves when above | `proof_clamp_min_preserves_when_above` | ✓ |
| clamp_max result <= max | `proof_clamp_max_result_le_max` | ✓ |
| clamp_max preserves when below | `proof_clamp_max_preserves_when_below` | ✓ |
| ClampExt clamp_to | `proof_clamp_ext_clamp_to` | ✓ |
| ClampExt clamp_min matches | `proof_clamp_ext_clamp_min_matches` | ✓ |
| ClampExt clamp_max matches | `proof_clamp_ext_clamp_max_matches` | ✓ |
| clamp_with_info value matches | `proof_clamp_with_info_value_matches_clamp` | ✓ |
| clamp_with_info not clamped | `proof_clamp_with_info_not_clamped` | ✓ |
| clamp_with_info clamped to min | `proof_clamp_with_info_clamped_to_min` | ✓ |
| clamp_with_info clamped to max | `proof_clamp_with_info_clamped_to_max` | ✓ |
| wrap_int result in range | `proof_wrap_int_result_in_range` | ✓ |
| wrap_int preserves value in range | `proof_wrap_int_preserves_value_in_range` | ✓ |
| Clamped new value in range | `proof_clamped_new_value_in_range` | ✓ |
| Clamped new matches clamp | `proof_clamped_new_matches_clamp` | ✓ |
| Clamped min/max accessors | `proof_clamped_min_max_accessors` | ✓ |
| Clamped set clamps value | `proof_clamped_set_clamps_value` | ✓ |
| Clamped is_at_min | `proof_clamped_is_at_min` | ✓ |
| Clamped is_at_max | `proof_clamped_is_at_max` | ✓ |
| Clamped invariant preserved | `proof_clamped_invariant_preserved` | ✓ |

### Type Conversion (`drbot-convert/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| TryConvert i64 to i32 valid range | `proof_try_convert_i64_to_i32_valid_range` | ✓ |
| TryConvert i64 to i32 overflow | `proof_try_convert_i64_to_i32_overflow` | ✓ |
| TryConvert i64 to i32 underflow | `proof_try_convert_i64_to_i32_underflow` | ✓ |
| TryConvert i64 to u8 valid range | `proof_try_convert_i64_to_u8_valid_range` | ✓ |
| TryConvert i64 to u8 negative | `proof_try_convert_i64_to_u8_negative` | ✓ |
| TryConvert u64 to i32 valid range | `proof_try_convert_u64_to_i32_valid_range` | ✓ |
| TryConvert u64 to i32 overflow | `proof_try_convert_u64_to_i32_overflow` | ✓ |
| convert_or_default valid value | `proof_convert_or_default_valid_value` | ✓ |
| convert_or_default overflow | `proof_convert_or_default_overflow_returns_default` | ✓ |
| convert_or_default underflow | `proof_convert_or_default_underflow_returns_default` | ✓ |
| convert_or valid value | `proof_convert_or_valid_value` | ✓ |
| convert_or invalid returns fallback | `proof_convert_or_invalid_returns_fallback` | ✓ |
| chain finish returns value | `proof_chain_finish_returns_value` | ✓ |
| chain then valid conversion | `proof_chain_then_valid_conversion` | ✓ |
| chain then invalid conversion | `proof_chain_then_invalid_conversion` | ✓ |
| SaturatingConvert i64 to i32 valid | `proof_saturating_convert_i64_to_i32_valid` | ✓ |
| SaturatingConvert i64 to i32 overflow | `proof_saturating_convert_i64_to_i32_overflow` | ✓ |
| SaturatingConvert i64 to i32 underflow | `proof_saturating_convert_i64_to_i32_underflow` | ✓ |
| SaturatingConvert u64 to u32 valid | `proof_saturating_convert_u64_to_u32_valid` | ✓ |
| SaturatingConvert u64 to u32 overflow | `proof_saturating_convert_u64_to_u32_overflow` | ✓ |
| SaturatingConvert result in range | `proof_saturating_convert_result_in_range` | ✓ |
| SaturatingConvert idempotent | `proof_saturating_convert_idempotent` | ✓ |
| LossyConvert i64 to i32 | `proof_lossy_convert_i64_to_i32` | ✓ |
| LossyConvert u64 to u32 | `proof_lossy_convert_u64_to_u32` | ✓ |
| LossyConvert preserves valid values | `proof_lossy_convert_preserves_valid_values` | ✓ |
| ConverterRegistry new is empty | `proof_registry_new_is_empty` | ✓ |
| ConverterRegistry default is empty | `proof_registry_default_is_empty` | ✓ |

### Base64 Utilities (`drbot-base64-util/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Standard alphabet size | `proof_standard_alphabet_size` | ✓ |
| URL-safe alphabet size | `proof_url_safe_alphabet_size` | ✓ |
| Standard alphabet unique | `proof_standard_alphabet_unique` | ✓ |
| URL-safe no + or / | `proof_url_safe_no_plus_slash` | ✓ |
| Default config is standard | `proof_default_config_is_standard` | ✓ |
| Standard config has padding | `proof_standard_config_has_padding` | ✓ |
| Standard no-pad config | `proof_standard_no_pad_config` | ✓ |
| URL-safe config | `proof_url_safe_config` | ✓ |
| URL-safe no-pad config | `proof_url_safe_no_pad_config` | ✓ |
| Encode empty | `proof_encode_empty` | ✓ |
| Encode one byte length | `proof_encode_one_byte_length` | ✓ |
| Encode two bytes length | `proof_encode_two_bytes_length` | ✓ |
| Encode three bytes length | `proof_encode_three_bytes_length` | ✓ |
| Encode no-pad one byte | `proof_encode_no_pad_one_byte` | ✓ |
| Encode no-pad two bytes | `proof_encode_no_pad_two_bytes` | ✓ |
| Encode uses valid chars | `proof_encode_uses_valid_chars` | ✓ |
| Encode URL-safe no special | `proof_encode_url_safe_no_special` | ✓ |
| Six-bit mask | `proof_six_bit_mask` | ✓ |
| Three-byte packing | `proof_three_byte_packing` | ✓ |
| One-byte packing | `proof_one_byte_packing` | ✓ |
| Two-byte packing | `proof_two_byte_packing` | ✓ |
| Decode first byte reconstruction | `proof_decode_first_byte_reconstruction` | ✓ |
| Decode second byte reconstruction | `proof_decode_second_byte_reconstruction` | ✓ |
| Decode third byte reconstruction | `proof_decode_third_byte_reconstruction` | ✓ |
| Base64 standard default | `proof_base64_standard_default` | ✓ |
| Base64 standard constructor | `proof_base64_standard_constructor` | ✓ |
| Base64 URL-safe constructor | `proof_base64_url_safe_constructor` | ✓ |
| Base64 encode matches function | `proof_base64_encode_matches_function` | ✓ |

### Borrow Extensions (`drbot-borrow-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| borrow_with applies function | `proof_borrow_with_applies_function` | ✓ |
| borrow_with identity | `proof_borrow_with_identity` | ✓ |
| borrow_mut_with applies function | `proof_borrow_mut_with_applies_function` | ✓ |
| borrow_mut_with modifies | `proof_borrow_mut_with_modifies` | ✓ |
| Borrowed is_borrowed | `proof_borrowed_is_borrowed` | ✓ |
| Borrowed owned is_owned | `proof_borrowed_owned_is_owned` | ✓ |
| Borrowed as_ref borrowed | `proof_borrowed_as_ref_borrowed` | ✓ |
| Borrowed as_ref owned | `proof_borrowed_as_ref_owned` | ✓ |
| Borrowed into_owned from borrowed | `proof_borrowed_into_owned_from_borrowed` | ✓ |
| Borrowed into_owned from owned | `proof_borrowed_into_owned_from_owned` | ✓ |
| Borrowed to_mut converts to owned | `proof_borrowed_to_mut_converts_to_owned` | ✓ |
| Borrowed to_mut modifiable | `proof_borrowed_to_mut_modifiable` | ✓ |
| Borrowed deref | `proof_borrowed_deref` | ✓ |
| Borrowed owned deref | `proof_borrowed_owned_deref` | ✓ |
| TransparentBorrow new | `proof_transparent_borrow_new` | ✓ |
| TransparentBorrow into_inner | `proof_transparent_borrow_into_inner` | ✓ |
| TransparentBorrow borrow | `proof_transparent_borrow_borrow` | ✓ |
| TransparentBorrow borrow_mut | `proof_transparent_borrow_borrow_mut` | ✓ |
| TransparentBorrow as_ref | `proof_transparent_borrow_as_ref` | ✓ |
| TransparentBorrow as_mut | `proof_transparent_borrow_as_mut` | ✓ |
| TransparentBorrow deref | `proof_transparent_borrow_deref` | ✓ |
| TransparentBorrow deref_mut | `proof_transparent_borrow_deref_mut` | ✓ |
| BorrowGuard new | `proof_borrow_guard_new` | ✓ |
| BorrowGuard deref | `proof_borrow_guard_deref` | ✓ |
| BorrowMutGuard new | `proof_borrow_mut_guard_new` | ✓ |
| BorrowMutGuard get_mut | `proof_borrow_mut_guard_get_mut` | ✓ |
| BorrowMutGuard deref | `proof_borrow_mut_guard_deref` | ✓ |
| BorrowMutGuard deref_mut | `proof_borrow_mut_guard_deref_mut` | ✓ |
| map_ref applies function | `proof_map_ref_applies_function` | ✓ |
| map_mut applies function | `proof_map_mut_applies_function` | ✓ |

### Box Extensions (`drbot-box-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| boxed stores value | `proof_boxed_stores_value` | ✓ |
| try_box succeeds | `proof_try_box_succeeds` | ✓ |
| clone_box clones value | `proof_clone_box_clones_value` | ✓ |
| map_box applies function | `proof_map_box_applies_function` | ✓ |
| map_box identity | `proof_map_box_identity` | ✓ |
| flatten_box | `proof_flatten_box` | ✓ |
| SizedBox new | `proof_sized_box_new` | ✓ |
| SizedBox size u8 | `proof_sized_box_size_u8` | ✓ |
| SizedBox size u32 | `proof_sized_box_size_u32` | ✓ |
| SizedBox size u64 | `proof_sized_box_size_u64` | ✓ |
| SizedBox get | `proof_sized_box_get` | ✓ |
| SizedBox get_mut | `proof_sized_box_get_mut` | ✓ |
| SizedBox into_inner | `proof_sized_box_into_inner` | ✓ |
| SizedBox deref | `proof_sized_box_deref` | ✓ |
| SizedBox deref_mut | `proof_sized_box_deref_mut` | ✓ |
| MetaBox new | `proof_meta_box_new` | ✓ |
| MetaBox get | `proof_meta_box_get` | ✓ |
| MetaBox get_mut | `proof_meta_box_get_mut` | ✓ |
| MetaBox metadata | `proof_meta_box_metadata` | ✓ |
| MetaBox metadata_mut | `proof_meta_box_metadata_mut` | ✓ |
| MetaBox into_parts | `proof_meta_box_into_parts` | ✓ |
| MetaBox deref | `proof_meta_box_deref` | ✓ |
| MetaBox deref_mut | `proof_meta_box_deref_mut` | ✓ |
| LazyBox not initialized initially | `proof_lazy_box_not_initialized_initially` | ✓ |
| LazyBox initialized after get | `proof_lazy_box_initialized_after_get` | ✓ |
| LazyBox get returns value | `proof_lazy_box_get_returns_value` | ✓ |
| LazyBox get_mut returns value | `proof_lazy_box_get_mut_returns_value` | ✓ |
| LazyBox get_mut modifiable | `proof_lazy_box_get_mut_modifiable` | ✓ |
| LazyBox initialized after get_mut | `proof_lazy_box_initialized_after_get_mut` | ✓ |

### Assertion Utilities (`drbot-assertion/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| assert_that true ok | `proof_assert_that_true_ok` | ✓ |
| assert_that false err | `proof_assert_that_false_err` | ✓ |
| assert_eq equal ok | `proof_assert_eq_equal_ok` | ✓ |
| assert_eq not equal err | `proof_assert_eq_not_equal_err` | ✓ |
| assert_ne not equal ok | `proof_assert_ne_not_equal_ok` | ✓ |
| assert_ne equal err | `proof_assert_ne_equal_err` | ✓ |
| assert_lt less ok | `proof_assert_lt_less_ok` | ✓ |
| assert_lt ge err | `proof_assert_lt_ge_err` | ✓ |
| assert_le le ok | `proof_assert_le_le_ok` | ✓ |
| assert_le gt err | `proof_assert_le_gt_err` | ✓ |
| assert_gt greater ok | `proof_assert_gt_greater_ok` | ✓ |
| assert_gt le err | `proof_assert_gt_le_err` | ✓ |
| assert_ge ge ok | `proof_assert_ge_ge_ok` | ✓ |
| assert_ge lt err | `proof_assert_ge_lt_err` | ✓ |
| assert_some some ok | `proof_assert_some_some_ok` | ✓ |
| assert_some none err | `proof_assert_some_none_err` | ✓ |
| assert_none none ok | `proof_assert_none_none_ok` | ✓ |
| assert_none some err | `proof_assert_none_some_err` | ✓ |
| assert_ok ok ok | `proof_assert_ok_ok_ok` | ✓ |
| assert_ok err err | `proof_assert_ok_err_err` | ✓ |
| assert_err err ok | `proof_assert_err_err_ok` | ✓ |
| assert_err ok err | `proof_assert_err_ok_err` | ✓ |
| AssertionResult passed | `proof_assertion_result_passed` | ✓ |
| AssertionResult failed | `proof_assertion_result_failed` | ✓ |
| AssertionResult at sets location | `proof_assertion_result_at_sets_location` | ✓ |
| Precondition check true ok | `proof_precondition_check_true_ok` | ✓ |
| Precondition check false err | `proof_precondition_check_false_err` | ✓ |
| Precondition require some ok | `proof_precondition_require_some_ok` | ✓ |
| Precondition require none err | `proof_precondition_require_none_err` | ✓ |
| Precondition require_in_range ok | `proof_precondition_require_in_range_ok` | ✓ |
| Precondition require_in_range err | `proof_precondition_require_in_range_err` | ✓ |
| Postcondition check true ok | `proof_postcondition_check_true_ok` | ✓ |
| Postcondition check false err | `proof_postcondition_check_false_err` | ✓ |
| Postcondition ensure pass | `proof_postcondition_ensure_pass` | ✓ |
| Postcondition ensure fail | `proof_postcondition_ensure_fail` | ✓ |
| AssertionCollector new empty | `proof_collector_new_empty` | ✓ |
| AssertionCollector default empty | `proof_collector_default_empty` | ✓ |
| AssertionCollector add passed | `proof_collector_add_passed_increments_pass` | ✓ |
| AssertionCollector add failed | `proof_collector_add_failed_increments_fail` | ✓ |
| AssertionCollector total is sum | `proof_collector_total_is_sum` | ✓ |
| AssertionCollector all_passed false | `proof_collector_all_passed_false_with_failures` | ✓ |
| AssertionCollector all_passed true | `proof_collector_all_passed_true_no_failures` | ✓ |

### Assumption Utilities (`drbot-assume/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| AssumptionState pending distinct | `proof_assumption_state_pending_distinct` | ✓ |
| AssumptionState equality reflexive | `proof_assumption_state_equality_reflexive` | ✓ |
| assume true ok | `proof_assume_true_ok` | ✓ |
| assume false err | `proof_assume_false_err` | ✓ |
| assume_with_reason true ok | `proof_assume_with_reason_true_ok` | ✓ |
| assume_with_reason false err | `proof_assume_with_reason_false_err` | ✓ |
| Assumptions new empty | `proof_assumptions_new_empty` | ✓ |
| Assumptions default empty | `proof_assumptions_default_empty` | ✓ |
| Assumptions assume sets pending | `proof_assumptions_assume_sets_pending` | ✓ |
| Assumptions validate sets valid | `proof_assumptions_validate_sets_valid` | ✓ |
| Assumptions validate unknown err | `proof_assumptions_validate_unknown_err` | ✓ |
| Assumptions invalidate sets invalid | `proof_assumptions_invalidate_sets_invalid` | ✓ |
| Assumptions invalidate unknown err | `proof_assumptions_invalidate_unknown_err` | ✓ |
| Assumptions clear removes all | `proof_assumptions_clear_removes_all` | ✓ |
| Assumptions assert_all_valid empty ok | `proof_assumptions_assert_all_valid_empty_ok` | ✓ |
| Assumptions assert_all_valid pending err | `proof_assumptions_assert_all_valid_pending_err` | ✓ |
| Assumptions assert_all_valid all valid ok | `proof_assumptions_assert_all_valid_all_valid_ok` | ✓ |
| Assumptions assert_all_valid invalid err | `proof_assumptions_assert_all_valid_invalid_err` | ✓ |
| AssumeBuilder new empty | `proof_assume_builder_new_empty` | ✓ |
| AssumeBuilder default empty | `proof_assume_builder_default_empty` | ✓ |
| AssumeBuilder that true ok | `proof_assume_builder_that_true_ok` | ✓ |
| AssumeBuilder that false err | `proof_assume_builder_that_false_err` | ✓ |
| AssumeBuilder all true ok | `proof_assume_builder_all_true_ok` | ✓ |
| AssumeBuilder any false err | `proof_assume_builder_any_false_err` | ✓ |
| AssumeBuilder violations empty | `proof_assume_builder_violations_empty_when_all_true` | ✓ |
| AssumeBuilder violations count | `proof_assume_builder_violations_count` | ✓ |
| assuming returns builder | `proof_assuming_returns_builder` | ✓ |

### Any Extensions (`drbot-any-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| AnyExt is_type same type | `proof_any_ext_is_type_same_type` | ✓ |
| AnyExt is_type different type | `proof_any_ext_is_type_different_type` | ✓ |
| AnyExt try_ref same type | `proof_any_ext_try_ref_same_type` | ✓ |
| AnyExt try_ref different type | `proof_any_ext_try_ref_different_type` | ✓ |
| AnyExt try_mut same type | `proof_any_ext_try_mut_same_type` | ✓ |
| AnyExt try_mut modifiable | `proof_any_ext_try_mut_modifiable` | ✓ |
| AnyBox new stores value | `proof_any_box_new_stores_value` | ✓ |
| AnyBox is correct type | `proof_any_box_is_correct_type` | ✓ |
| AnyBox downcast_ref correct type | `proof_any_box_downcast_ref_correct_type` | ✓ |
| AnyBox downcast_ref wrong type | `proof_any_box_downcast_ref_wrong_type` | ✓ |
| AnyBox downcast_mut modifiable | `proof_any_box_downcast_mut_modifiable` | ✓ |
| AnyBox downcast success | `proof_any_box_downcast_success` | ✓ |
| AnyBox downcast failure | `proof_any_box_downcast_failure` | ✓ |
| AnyBox get_ref success | `proof_any_box_get_ref_success` | ✓ |
| AnyBox get_ref failure | `proof_any_box_get_ref_failure` | ✓ |
| AnyBox get_mut success | `proof_any_box_get_mut_success` | ✓ |
| AnyBox get_mut failure | `proof_any_box_get_mut_failure` | ✓ |
| AnyOption none is_none | `proof_any_option_none_is_none` | ✓ |
| AnyOption some is_some | `proof_any_option_some_is_some` | ✓ |
| AnyOption default is none | `proof_any_option_default_is_none` | ✓ |
| AnyOption set makes some | `proof_any_option_set_makes_some` | ✓ |
| AnyOption clear makes none | `proof_any_option_clear_makes_none` | ✓ |
| AnyOption get_ref some | `proof_any_option_get_ref_some` | ✓ |
| AnyOption get_ref none | `proof_any_option_get_ref_none` | ✓ |
| AnyOption get_ref wrong type | `proof_any_option_get_ref_wrong_type` | ✓ |
| AnyOption take success | `proof_any_option_take_success` | ✓ |
| AnyOption take none | `proof_any_option_take_none` | ✓ |
| is_type correct | `proof_is_type_correct` | ✓ |
| cast_ref correct | `proof_cast_ref_correct` | ✓ |
| cast_ref wrong | `proof_cast_ref_wrong` | ✓ |
| cast_mut modifiable | `proof_cast_mut_modifiable` | ✓ |

### Comparison Utilities (`drbot-cmp/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| compare reflexive | `proof_compare_reflexive` | ✓ |
| compare antisymmetric | `proof_compare_antisymmetric` | ✓ |
| compare less | `proof_compare_less` | ✓ |
| compare greater | `proof_compare_greater` | ✓ |
| compare equal | `proof_compare_equal` | ✓ |
| compare_by_key reflexive | `proof_compare_by_key_reflexive` | ✓ |
| reverse_ordering Less | `proof_reverse_ordering_less` | ✓ |
| reverse_ordering Equal | `proof_reverse_ordering_equal` | ✓ |
| reverse_ordering Greater | `proof_reverse_ordering_greater` | ✓ |
| reverse_ordering involution | `proof_reverse_ordering_involution` | ✓ |
| then_ordering Less first | `proof_then_ordering_less_first` | ✓ |
| then_ordering Greater first | `proof_then_ordering_greater_first` | ✓ |
| then_ordering Equal uses second | `proof_then_ordering_equal_uses_second` | ✓ |
| Comparator lt consistent | `proof_comparator_lt_consistent` | ✓ |
| Comparator le consistent | `proof_comparator_le_consistent` | ✓ |
| Comparator gt consistent | `proof_comparator_gt_consistent` | ✓ |
| Comparator ge consistent | `proof_comparator_ge_consistent` | ✓ |
| Comparator eq consistent | `proof_comparator_eq_consistent` | ✓ |
| Comparator lt gt exclusive | `proof_comparator_lt_gt_exclusive` | ✓ |
| Comparator le ge overlap eq | `proof_comparator_le_ge_overlap_eq` | ✓ |
| ThreeWay from Ordering Less | `proof_threeway_from_ordering_less` | ✓ |
| ThreeWay from Ordering Equal | `proof_threeway_from_ordering_equal` | ✓ |
| ThreeWay from Ordering Greater | `proof_threeway_from_ordering_greater` | ✓ |
| ThreeWay to Ordering Less | `proof_threeway_to_ordering_less` | ✓ |
| ThreeWay to Ordering Equal | `proof_threeway_to_ordering_equal` | ✓ |
| ThreeWay to Ordering Greater | `proof_threeway_to_ordering_greater` | ✓ |
| ThreeWay roundtrip | `proof_threeway_roundtrip` | ✓ |
| ThreeWay distinct | `proof_threeway_distinct` | ✓ |
| between inclusive min | `proof_between_inclusive_min` | ✓ |
| between inclusive max | `proof_between_inclusive_max` | ✓ |
| between middle | `proof_between_middle` | ✓ |
| between below min | `proof_between_below_min` | ✓ |
| between above max | `proof_between_above_max` | ✓ |
| strictly_between excludes min | `proof_strictly_between_excludes_min` | ✓ |
| strictly_between excludes max | `proof_strictly_between_excludes_max` | ✓ |
| strictly_between middle | `proof_strictly_between_middle` | ✓ |
| strictly_between implies between | `proof_strictly_between_implies_between` | ✓ |
| ChainedComparator empty equal | `proof_chained_comparator_empty_equal` | ✓ |
| ChainedComparator default equal | `proof_chained_comparator_default_equal` | ✓ |

### Default Extensions (`drbot-default-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| default_if true | `proof_default_if_true` | ✓ |
| default_if false | `proof_default_if_false` | ✓ |
| default_or returns value | `proof_default_or_returns_value` | ✓ |
| is_default zero | `proof_is_default_zero` | ✓ |
| is_default nonzero | `proof_is_default_nonzero` | ✓ |
| take_default returns old | `proof_take_default_returns_old` | ✓ |
| OrDefault Some | `proof_or_default_some` | ✓ |
| OrDefault None | `proof_or_default_none` | ✓ |
| WithDefault new builds default | `proof_with_default_new_builds_default` | ✓ |
| WithDefault default builds default | `proof_with_default_default_builds_default` | ✓ |
| WithDefault set value | `proof_with_default_set_value` | ✓ |
| WithDefault set_if true | `proof_with_default_set_if_true` | ✓ |
| WithDefault set_if false | `proof_with_default_set_if_false` | ✓ |
| WithDefault build_or | `proof_with_default_build_or` | ✓ |
| WithDefault set overrides build_or | `proof_with_default_set_overrides_build_or` | ✓ |
| DefaultRegistry new empty | `proof_default_registry_new_empty` | ✓ |
| DefaultRegistry default empty | `proof_default_registry_default_empty` | ✓ |
| DefaultRegistry register get | `proof_default_registry_register_get` | ✓ |
| DefaultRegistry has | `proof_default_registry_has` | ✓ |
| DefaultRegistry get_or | `proof_default_registry_get_or` | ✓ |
| DefaultRegistry get_or with value | `proof_default_registry_get_or_with_value` | ✓ |
| Defaultable new is_default | `proof_defaultable_new_is_default` | ✓ |
| Defaultable set not default | `proof_defaultable_set_not_default` | ✓ |
| Defaultable clear restores default | `proof_defaultable_clear_restores_default` | ✓ |
| Defaultable default_value | `proof_defaultable_default_value` | ✓ |
| LazyDefault not initialized | `proof_lazy_default_not_initialized` | ✓ |
| LazyDefault get initializes | `proof_lazy_default_get_initializes` | ✓ |
| LazyDefault get_mut initializes | `proof_lazy_default_get_mut_initializes` | ✓ |
| LazyDefault get idempotent | `proof_lazy_default_get_idempotent` | ✓ |

### Function Utilities (`drbot-fn-util/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| identity preserves value | `proof_identity_preserves_value` | ✓ |
| identity idempotent | `proof_identity_idempotent` | ✓ |
| constant returns same | `proof_constant_returns_same` | ✓ |
| constant multiple calls | `proof_constant_multiple_calls` | ✓ |
| compose identity left | `proof_compose_identity_left` | ✓ |
| compose identity right | `proof_compose_identity_right` | ✓ |
| compose application order | `proof_compose_application_order` | ✓ |
| flip swaps args | `proof_flip_swaps_args` | ✓ |
| flip double is identity | `proof_flip_double_is_identity` | ✓ |
| apply calls function | `proof_apply_calls_function` | ✓ |
| apply identity | `proof_apply_identity` | ✓ |
| always ignores arg | `proof_always_ignores_arg` | ✓ |
| always constant across args | `proof_always_constant_across_args` | ✓ |
| negate inverts true | `proof_negate_inverts_true` | ✓ |
| negate inverts false | `proof_negate_inverts_false` | ✓ |
| negate double is original | `proof_negate_double_is_original` | ✓ |
| tap returns value | `proof_tap_returns_value` | ✓ |
| tap preserves value | `proof_tap_preserves_value` | ✓ |
| tap_mut returns modified | `proof_tap_mut_returns_modified` | ✓ |
| also returns value | `proof_also_returns_value` | ✓ |
| let_in transforms | `proof_let_in_transforms` | ✓ |
| let_in identity | `proof_let_in_identity` | ✓ |
| take_if true Some | `proof_take_if_true_some` | ✓ |
| take_if false None | `proof_take_if_false_none` | ✓ |
| take_if predicate | `proof_take_if_predicate` | ✓ |
| take_unless true None | `proof_take_unless_true_none` | ✓ |
| take_unless false Some | `proof_take_unless_false_some` | ✓ |
| take_unless opposite of take_if | `proof_take_unless_opposite_of_take_if` | ✓ |
| Memoized new empty cache | `proof_memoized_new_empty_cache` | ✓ |
| Memoized call returns result | `proof_memoized_call_returns_result` | ✓ |
| Memoized call caches | `proof_memoized_call_caches` | ✓ |
| Memoized same input same output | `proof_memoized_same_input_same_output` | ✓ |
| Memoized clear cache | `proof_memoized_clear_cache` | ✓ |
| Once new not computed | `proof_once_new_not_computed` | ✓ |
| Once get computes | `proof_once_get_computes` | ✓ |
| Once get idempotent | `proof_once_get_idempotent` | ✓ |
| memoize creates Memoized | `proof_memoize_creates_memoized` | ✓ |

### Math Utilities (`drbot-math/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| clamp within bounds | `proof_clamp_within_bounds` | ✓ |
| clamp preserves in range | `proof_clamp_preserves_in_range` | ✓ |
| clamp below min | `proof_clamp_below_min` | ✓ |
| clamp above max | `proof_clamp_above_max` | ✓ |
| clamp idempotent | `proof_clamp_idempotent` | ✓ |
| gcd zero b | `proof_gcd_zero_b` | ✓ |
| gcd commutative | `proof_gcd_commutative` | ✓ |
| gcd divides both | `proof_gcd_divides_both` | ✓ |
| gcd same value | `proof_gcd_same_value` | ✓ |
| lcm zero | `proof_lcm_zero` | ✓ |
| lcm commutative | `proof_lcm_commutative` | ✓ |
| lcm divisible by both | `proof_lcm_divisible_by_both` | ✓ |
| lcm same value | `proof_lcm_same_value` | ✓ |
| gcd lcm product | `proof_gcd_lcm_product` | ✓ |
| factorial zero | `proof_factorial_zero` | ✓ |
| factorial one | `proof_factorial_one` | ✓ |
| factorial small | `proof_factorial_small` | ✓ |
| factorial overflow limit | `proof_factorial_overflow_limit` | ✓ |
| binomial k greater n | `proof_binomial_k_greater_n` | ✓ |
| binomial k zero | `proof_binomial_k_zero` | ✓ |
| binomial k equals n | `proof_binomial_k_equals_n` | ✓ |
| binomial symmetry | `proof_binomial_symmetry` | ✓ |
| is_prime zero one | `proof_is_prime_zero_one` | ✓ |
| is_prime two | `proof_is_prime_two` | ✓ |
| is_prime small primes | `proof_is_prime_small_primes` | ✓ |
| is_prime composites | `proof_is_prime_composites` | ✓ |
| fibonacci base cases | `proof_fibonacci_base_cases` | ✓ |
| fibonacci small | `proof_fibonacci_small` | ✓ |
| fibonacci overflow limit | `proof_fibonacci_overflow_limit` | ✓ |
| mod_pow modulo one | `proof_mod_pow_modulo_one` | ✓ |
| mod_pow exp zero | `proof_mod_pow_exp_zero` | ✓ |
| mod_pow exp one | `proof_mod_pow_exp_one` | ✓ |
| mod_pow result bounded | `proof_mod_pow_result_bounded` | ✓ |
| sign positive | `proof_sign_positive` | ✓ |
| sign negative | `proof_sign_negative` | ✓ |
| sign zero | `proof_sign_zero` | ✓ |
| approx_eq same | `proof_approx_eq_same` | ✓ |
| approx_eq within epsilon | `proof_approx_eq_within_epsilon` | ✓ |
| approx_eq outside epsilon | `proof_approx_eq_outside_epsilon` | ✓ |
| approx_eq symmetric | `proof_approx_eq_symmetric` | ✓ |

### Ordering Extensions (`drbot-ord-ext/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| OrderingExt is_less | `proof_ordering_ext_is_less` | ✓ |
| OrderingExt is_equal | `proof_ordering_ext_is_equal` | ✓ |
| OrderingExt is_greater | `proof_ordering_ext_is_greater` | ✓ |
| OrderingExt is_le | `proof_ordering_ext_is_le` | ✓ |
| OrderingExt is_ge | `proof_ordering_ext_is_ge` | ✓ |
| OrderingExt to_i8 | `proof_ordering_ext_to_i8` | ✓ |
| OrderingExt from_i8 negative | `proof_ordering_ext_from_i8_negative` | ✓ |
| OrderingExt from_i8 zero | `proof_ordering_ext_from_i8_zero` | ✓ |
| OrderingExt from_i8 positive | `proof_ordering_ext_from_i8_positive` | ✓ |
| OrderingExt roundtrip | `proof_ordering_ext_roundtrip` | ✓ |
| OrdExt max_of | `proof_ord_ext_max_of` | ✓ |
| OrdExt min_of | `proof_ord_ext_min_of` | ✓ |
| OrdExt clamp_to bounds | `proof_ord_ext_clamp_to_bounds` | ✓ |
| OrdExt clamp_to preserves | `proof_ord_ext_clamp_to_preserves` | ✓ |
| OrdExt in_range true | `proof_ord_ext_in_range_true` | ✓ |
| OrdExt in_range false below | `proof_ord_ext_in_range_false_below` | ✓ |
| OrdExt in_range false above | `proof_ord_ext_in_range_false_above` | ✓ |
| OrdExt compare_to | `proof_ord_ext_compare_to` | ✓ |
| PartialOrdExt try_max comparable | `proof_partial_ord_ext_try_max_comparable` | ✓ |
| PartialOrdExt try_min comparable | `proof_partial_ord_ext_try_min_comparable` | ✓ |
| PartialOrdExt is_comparable i8 | `proof_partial_ord_ext_is_comparable_i8` | ✓ |
| PartialOrdExt is_lt | `proof_partial_ord_ext_is_lt` | ✓ |
| PartialOrdExt is_gt | `proof_partial_ord_ext_is_gt` | ✓ |
| OrderingBuilder new equal | `proof_ordering_builder_new_equal` | ✓ |
| OrderingBuilder default equal | `proof_ordering_builder_default_equal` | ✓ |
| OrderingBuilder compare less | `proof_ordering_builder_compare_less` | ✓ |
| OrderingBuilder compare greater | `proof_ordering_builder_compare_greater` | ✓ |
| OrderingBuilder compare equal | `proof_ordering_builder_compare_equal` | ✓ |
| OrderingBuilder then used when equal | `proof_ordering_builder_then_used_when_equal` | ✓ |
| OrderingBuilder then ignored when not equal | `proof_ordering_builder_then_ignored_when_not_equal` | ✓ |
| OrderingBuilder chain | `proof_ordering_builder_chain` | ✓ |
| reverse less | `proof_reverse_less` | ✓ |
| reverse equal | `proof_reverse_equal` | ✓ |
| reverse greater | `proof_reverse_greater` | ✓ |
| reverse involution | `proof_reverse_involution` | ✓ |
| from_cmp consistent | `proof_from_cmp_consistent` | ✓ |
| from_bool true | `proof_from_bool_true` | ✓ |
| from_bool false | `proof_from_bool_false` | ✓ |

### Lazy Evaluation (`drbot-lazy/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| Lazy not initialized initially | `proof_lazy_not_initialized_initially` | ✓ |
| Lazy get initializes | `proof_lazy_get_initializes` | ✓ |
| Lazy get returns value | `proof_lazy_get_returns_value` | ✓ |
| Lazy get idempotent | `proof_lazy_get_idempotent` | ✓ |
| Lazy get_if_initialized before | `proof_lazy_get_if_initialized_before` | ✓ |
| Lazy get_if_initialized after | `proof_lazy_get_if_initialized_after` | ✓ |
| Lazy force initializes | `proof_lazy_force_initializes` | ✓ |
| LazyResult not initialized initially | `proof_lazy_result_not_initialized_initially` | ✓ |
| LazyResult get initializes | `proof_lazy_result_get_initializes` | ✓ |
| LazyResult get ok | `proof_lazy_result_get_ok` | ✓ |
| LazyResult get err | `proof_lazy_result_get_err` | ✓ |
| LazyResult get idempotent | `proof_lazy_result_get_idempotent` | ✓ |
| LazyCell not initialized initially | `proof_lazy_cell_not_initialized_initially` | ✓ |
| LazyCell get initializes | `proof_lazy_cell_get_initializes` | ✓ |
| LazyCell get returns value | `proof_lazy_cell_get_returns_value` | ✓ |
| LazyCell get idempotent | `proof_lazy_cell_get_idempotent` | ✓ |
| Deferred eval returns value | `proof_deferred_eval_returns_value` | ✓ |
| Deferred eval multiple times | `proof_deferred_eval_multiple_times` | ✓ |
| Deferred map transforms | `proof_deferred_map_transforms` | ✓ |
| Deferred map chain | `proof_deferred_map_chain` | ✓ |
| LazySeq get in bounds | `proof_lazy_seq_get_in_bounds` | ✓ |
| LazySeq get out of bounds | `proof_lazy_seq_get_out_of_bounds` | ✓ |
| LazySeq from_vec get | `proof_lazy_seq_from_vec_get` | ✓ |
| LazySeq take length | `proof_lazy_seq_take_length` | ✓ |
| LazySeq take bounded | `proof_lazy_seq_take_bounded` | ✓ |
| lazy function | `proof_lazy_function` | ✓ |
| defer function | `proof_defer_function` | ✓ |

### Guard Patterns (`drbot-guard/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ScopeGuard new active | `proof_scope_guard_new_active` | ✓ |
| ScopeGuard dismiss deactivates | `proof_scope_guard_dismiss_deactivates` | ✓ |
| ScopeGuard cancel returns cleanup | `proof_scope_guard_cancel_returns_cleanup` | ✓ |
| SuccessGuard new not success | `proof_success_guard_new_not_success` | ✓ |
| FailureGuard new not committed | `proof_failure_guard_new_not_committed` | ✓ |
| ValueGuard get | `proof_value_guard_get` | ✓ |
| ValueGuard get_mut | `proof_value_guard_get_mut` | ✓ |
| ValueGuard take | `proof_value_guard_take` | ✓ |
| ValueGuard deref | `proof_value_guard_deref` | ✓ |
| ValueGuard deref_mut | `proof_value_guard_deref_mut` | ✓ |
| RefGuard new valid | `proof_ref_guard_new_valid` | ✓ |
| RefGuard default valid | `proof_ref_guard_default_valid` | ✓ |
| RefGuard invalidate | `proof_ref_guard_invalidate` | ✓ |
| RefGuard weak shares state | `proof_ref_guard_weak_shares_state` | ✓ |
| WeakGuard clone | `proof_weak_guard_clone` | ✓ |
| ReentrancyGuard new unlocked | `proof_reentrancy_guard_new_unlocked` | ✓ |
| ReentrancyGuard default unlocked | `proof_reentrancy_guard_default_unlocked` | ✓ |
| ReentrancyGuard try_enter success | `proof_reentrancy_guard_try_enter_success` | ✓ |
| ReentrancyGuard try_enter fails when locked | `proof_reentrancy_guard_try_enter_fails_when_locked` | ✓ |
| BoolGuard set_true config | `proof_bool_guard_set_true_config` | ✓ |
| BoolGuard set_false config | `proof_bool_guard_set_false_config` | ✓ |
| CounterGuard increments on new | `proof_counter_guard_increments_on_new` | ✓ |
| TimedMutex new | `proof_timed_mutex_new` | ✓ |
| TimedMutex guard deref | `proof_timed_mutex_guard_deref` | ✓ |
| TimedMutex guard deref_mut | `proof_timed_mutex_guard_deref_mut` | ✓ |
| defer creates scope guard | `proof_defer_creates_scope_guard` | ✓ |

### Signal/Slot Pattern (`drbot-slot/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| ConnectionId unique | `proof_connection_id_unique` | ✓ |
| ConnectionId default unique | `proof_connection_id_default_unique` | ✓ |
| ConnectionId equality | `proof_connection_id_equality` | ✓ |
| Signal new no connections | `proof_signal_new_no_connections` | ✓ |
| Signal default no connections | `proof_signal_default_no_connections` | ✓ |
| Signal connect adds connection | `proof_signal_connect_adds_connection` | ✓ |
| Signal disconnect_all clears | `proof_signal_disconnect_all_clears` | ✓ |
| Signal disconnect by id | `proof_signal_disconnect_by_id` | ✓ |
| Signal disconnect nonexistent | `proof_signal_disconnect_nonexistent` | ✓ |
| Signal clone shares slots | `proof_signal_clone_shares_slots` | ✓ |
| Signal0 new | `proof_signal0_new` | ✓ |
| Signal0 default | `proof_signal0_default` | ✓ |
| Signal2 new | `proof_signal2_new` | ✓ |
| Signal2 default | `proof_signal2_default` | ✓ |
| Connection id retrieval | `proof_connection_id_retrieval` | ✓ |
| ScopedConnection new | `proof_scoped_connection_new` | ✓ |
| ScopedConnection release | `proof_scoped_connection_release` | ✓ |
| ConnectionGroup new empty | `proof_connection_group_new_empty` | ✓ |
| ConnectionGroup default empty | `proof_connection_group_default_empty` | ✓ |
| ConnectionGroup add | `proof_connection_group_add` | ✓ |
| ConnectionGroup multiple add | `proof_connection_group_multiple_add` | ✓ |

### Unit Type Utilities (`drbot-unit/src/lib.rs`)

| Property | Proof | Status |
|----------|-------|--------|
| NamedUnit new | `proof_named_unit_new` | ✓ |
| NamedUnit default | `proof_named_unit_default` | ✓ |
| NamedUnit equality | `proof_named_unit_equality` | ✓ |
| Phantom new | `proof_phantom_new` | ✓ |
| Phantom equality | `proof_phantom_equality` | ✓ |
| Phantom default | `proof_phantom_default` | ✓ |
| Tagged new | `proof_tagged_new` | ✓ |
| Tagged into_inner | `proof_tagged_into_inner` | ✓ |
| Tagged get_mut | `proof_tagged_get_mut` | ✓ |
| Tagged map | `proof_tagged_map` | ✓ |
| Tagged retag | `proof_tagged_retag` | ✓ |
| Tagged default | `proof_tagged_default` | ✓ |
| Tagged equality | `proof_tagged_equality` | ✓ |
| Nothing default | `proof_nothing_default` | ✓ |
| Something default | `proof_something_default` | ✓ |
| Pending default | `proof_pending_default` | ✓ |
| Completed default | `proof_completed_default` | ✓ |
| Failed default | `proof_failed_default` | ✓ |
| Initialized default | `proof_initialized_default` | ✓ |
| Uninitialized default | `proof_uninitialized_default` | ✓ |
| TypeBool True | `proof_type_bool_true` | ✓ |
| TypeBool False | `proof_type_bool_false` | ✓ |
| True default | `proof_true_default` | ✓ |
| False default | `proof_false_default` | ✓ |
| Ignore always equal | `proof_ignore_always_equal` | ✓ |
| Ignore into_inner | `proof_ignore_into_inner` | ✓ |
| Ignore get | `proof_ignore_get` | ✓ |
| Ignore get_mut | `proof_ignore_get_mut` | ✓ |
| Ignore default | `proof_ignore_default` | ✓ |
| Align1 size zero | `proof_align1_size_zero` | ✓ |
| Align2 size zero | `proof_align2_size_zero` | ✓ |
| Align4 size zero | `proof_align4_size_zero` | ✓ |
| Align8 size zero | `proof_align8_size_zero` | ✓ |
| Zero default | `proof_zero_default` | ✓ |
| Succ default | `proof_succ_default` | ✓ |
| Type aliases | `proof_type_aliases` | ✓ |

### drbot-builder (31 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| BuilderField required unset | `proof_builder_field_required_unset` | ✓ |
| BuilderField required set | `proof_builder_field_required_set` | ✓ |
| BuilderField optional unset | `proof_builder_field_optional_unset` | ✓ |
| BuilderField with_default | `proof_builder_field_with_default` | ✓ |
| BuilderField take | `proof_builder_field_take` | ✓ |
| BuilderField take unset required | `proof_builder_field_take_unset_required` | ✓ |
| BuilderField take_optional | `proof_builder_field_take_optional` | ✓ |
| BuilderField take_optional unset | `proof_builder_field_take_optional_unset` | ✓ |
| StepBuilder new | `proof_step_builder_new` | ✓ |
| StepBuilder next | `proof_step_builder_next` | ✓ |
| StepBuilder chain | `proof_step_builder_chain` | ✓ |
| ConfigBuilder new empty | `proof_config_builder_new_empty` | ✓ |
| ConfigBuilder default empty | `proof_config_builder_default_empty` | ✓ |
| ConfigBuilder set/get | `proof_config_builder_set_get` | ✓ |
| ConfigBuilder set overwrite | `proof_config_builder_set_overwrite` | ✓ |
| ConfigBuilder multiple keys | `proof_config_builder_multiple_keys` | ✓ |
| ConfigBuilder into_config | `proof_config_builder_into_config` | ✓ |
| PersonBuilder new | `proof_person_builder_new` | ✓ |
| PersonBuilder name | `proof_person_builder_name` | ✓ |
| PersonBuilder age | `proof_person_builder_age` | ✓ |
| PersonBuilder email | `proof_person_builder_email` | ✓ |
| PersonBuilder build success | `proof_person_builder_build_success` | ✓ |
| PersonBuilder build with email | `proof_person_builder_build_with_email` | ✓ |
| PersonBuilder build missing name | `proof_person_builder_build_missing_name` | ✓ |
| PersonBuilder build missing age | `proof_person_builder_build_missing_age` | ✓ |
| PersonBuilder validate success | `proof_person_builder_validate_success` | ✓ |
| PersonBuilder validate missing name | `proof_person_builder_validate_missing_name` | ✓ |
| PersonBuilder validate missing age | `proof_person_builder_validate_missing_age` | ✓ |
| State Initial | `proof_state_initial` | ✓ |
| State Configured | `proof_state_configured` | ✓ |
| State Ready | `proof_state_ready` | ✓ |

### drbot-arc (28 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| TrackedArc new | `proof_tracked_arc_new` | ✓ |
| TrackedArc clone | `proof_tracked_arc_clone` | ✓ |
| TrackedArc multiple clones | `proof_tracked_arc_multiple_clones` | ✓ |
| TrackedArc is_unique | `proof_tracked_arc_is_unique` | ✓ |
| TrackedArc downgrade | `proof_tracked_arc_downgrade` | ✓ |
| TrackedArc get_mut unique | `proof_tracked_arc_get_mut_unique` | ✓ |
| TrackedArc get_mut not unique | `proof_tracked_arc_get_mut_not_unique` | ✓ |
| TrackedArc try_unwrap unique | `proof_tracked_arc_try_unwrap_unique` | ✓ |
| TrackedArc try_unwrap not unique | `proof_tracked_arc_try_unwrap_not_unique` | ✓ |
| WeakTracked upgrade success | `proof_weak_tracked_upgrade_success` | ✓ |
| WeakTracked upgrade fail | `proof_weak_tracked_upgrade_fail` | ✓ |
| WeakTracked clone | `proof_weak_tracked_clone` | ✓ |
| ArcSwap new/load | `proof_arc_swap_new_load` | ✓ |
| ArcSwap store | `proof_arc_swap_store` | ✓ |
| ArcSwap swap | `proof_arc_swap_swap` | ✓ |
| ArcSwap update | `proof_arc_swap_update` | ✓ |
| ArcList new empty | `proof_arc_list_new_empty` | ✓ |
| ArcList default empty | `proof_arc_list_default_empty` | ✓ |
| ArcList push | `proof_arc_list_push` | ✓ |
| ArcList get | `proof_arc_list_get` | ✓ |
| ArcList pop | `proof_arc_list_pop` | ✓ |
| ArcList pop empty | `proof_arc_list_pop_empty` | ✓ |
| ArcList clear | `proof_arc_list_clear` | ✓ |
| ArcList clone | `proof_arc_list_clone` | ✓ |
| arc_from_box | `proof_arc_from_box` | ✓ |
| arc_clone_inner | `proof_arc_clone_inner` | ✓ |
| arc_ptr_eq same | `proof_arc_ptr_eq_same` | ✓ |
| arc_ptr_eq different | `proof_arc_ptr_eq_different` | ✓ |

### drbot-cast (35 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| SafeCast i64→i32 in range | `proof_safe_cast_i64_to_i32_in_range` | ✓ |
| SafeCast i64→i32 overflow | `proof_safe_cast_i64_to_i32_overflow` | ✓ |
| SafeCast i64→i32 underflow | `proof_safe_cast_i64_to_i32_underflow` | ✓ |
| SafeCast i64→i16 in range | `proof_safe_cast_i64_to_i16_in_range` | ✓ |
| SafeCast i64→i16 overflow | `proof_safe_cast_i64_to_i16_overflow` | ✓ |
| SafeCast i64→i8 in range | `proof_safe_cast_i64_to_i8_in_range` | ✓ |
| SafeCast i64→i8 overflow | `proof_safe_cast_i64_to_i8_overflow` | ✓ |
| SafeCast i64→i8 underflow | `proof_safe_cast_i64_to_i8_underflow` | ✓ |
| SafeCast i64→u64 positive | `proof_safe_cast_i64_to_u64_positive` | ✓ |
| SafeCast i64→u64 negative | `proof_safe_cast_i64_to_u64_negative` | ✓ |
| SafeCast i64→u32 in range | `proof_safe_cast_i64_to_u32_in_range` | ✓ |
| SafeCast i64→u32 negative | `proof_safe_cast_i64_to_u32_negative` | ✓ |
| SafeCast i64→u32 overflow | `proof_safe_cast_i64_to_u32_overflow` | ✓ |
| SafeCast u64→i64 in range | `proof_safe_cast_u64_to_i64_in_range` | ✓ |
| SafeCast u64→i64 overflow | `proof_safe_cast_u64_to_i64_overflow` | ✓ |
| SafeCast u64→u32 in range | `proof_safe_cast_u64_to_u32_in_range` | ✓ |
| SafeCast u64→u32 overflow | `proof_safe_cast_u64_to_u32_overflow` | ✓ |
| CheckedCast success | `proof_checked_cast_success` | ✓ |
| CheckedCast failure | `proof_checked_cast_failure` | ✓ |
| UncheckedCast in range | `proof_unchecked_cast_in_range` | ✓ |
| UncheckedCast truncates | `proof_unchecked_cast_truncates` | ✓ |
| Saturate i64→i32 in range | `proof_saturate_i64_to_i32_in_range` | ✓ |
| Saturate i64→i32 overflow | `proof_saturate_i64_to_i32_overflow` | ✓ |
| Saturate i64→i32 underflow | `proof_saturate_i64_to_i32_underflow` | ✓ |
| Saturate u64→u32 in range | `proof_saturate_u64_to_u32_in_range` | ✓ |
| Saturate u64→u32 overflow | `proof_saturate_u64_to_u32_overflow` | ✓ |
| Saturate i64→u32 negative | `proof_saturate_i64_to_u32_negative` | ✓ |
| Saturate i64→u32 overflow | `proof_saturate_i64_to_u32_overflow` | ✓ |
| Saturate i32→u8 in range | `proof_saturate_i32_to_u8_in_range` | ✓ |
| Saturate i32→u8 negative | `proof_saturate_i32_to_u8_negative` | ✓ |
| Saturate i32→u8 overflow | `proof_saturate_i32_to_u8_overflow` | ✓ |
| cast helper | `proof_cast_helper` | ✓ |
| checked helper | `proof_checked_helper` | ✓ |
| unchecked helper | `proof_unchecked_helper` | ✓ |
| saturating helper | `proof_saturating_helper` | ✓ |

### drbot-atomic-ext (31 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| AtomicUsize inc | `proof_atomic_usize_inc` | ✓ |
| AtomicUsize dec | `proof_atomic_usize_dec` | ✓ |
| AtomicUsize add_get | `proof_atomic_usize_add_get` | ✓ |
| AtomicUsize get_set | `proof_atomic_usize_get_set` | ✓ |
| AtomicUsize update | `proof_atomic_usize_update` | ✓ |
| AtomicI64 inc | `proof_atomic_i64_inc` | ✓ |
| AtomicI64 dec | `proof_atomic_i64_dec` | ✓ |
| AtomicI64 get_set | `proof_atomic_i64_get_set` | ✓ |
| AtomicI64 update | `proof_atomic_i64_update` | ✓ |
| AtomicU64 inc | `proof_atomic_u64_inc` | ✓ |
| AtomicU64 dec | `proof_atomic_u64_dec` | ✓ |
| AtomicU64 get_set | `proof_atomic_u64_get_set` | ✓ |
| AtomicU64 fetch_max_get | `proof_atomic_u64_fetch_max_get` | ✓ |
| AtomicU64 fetch_min_get | `proof_atomic_u64_fetch_min_get` | ✓ |
| AtomicBool toggle false | `proof_atomic_bool_toggle_false` | ✓ |
| AtomicBool toggle true | `proof_atomic_bool_toggle_true` | ✓ |
| AtomicBool try_set false | `proof_atomic_bool_try_set_false` | ✓ |
| AtomicBool try_set true | `proof_atomic_bool_try_set_true` | ✓ |
| AtomicBool try_clear true | `proof_atomic_bool_try_clear_true` | ✓ |
| AtomicBool try_clear false | `proof_atomic_bool_try_clear_false` | ✓ |
| AtomicOption new is_none | `proof_atomic_option_new_is_none` | ✓ |
| AtomicOption default is_none | `proof_atomic_option_default_is_none` | ✓ |
| AtomicOption with is_some | `proof_atomic_option_with_is_some` | ✓ |
| AtomicOption store on empty | `proof_atomic_option_store_on_empty` | ✓ |
| AtomicOption store on existing | `proof_atomic_option_store_on_existing` | ✓ |
| AtomicOption take empty | `proof_atomic_option_take_empty` | ✓ |
| AtomicOption take existing | `proof_atomic_option_take_existing` | ✓ |
| AtomicIdGenerator new | `proof_atomic_id_generator_new` | ✓ |
| AtomicIdGenerator default | `proof_atomic_id_generator_default` | ✓ |
| AtomicIdGenerator next | `proof_atomic_id_generator_next` | ✓ |
| AtomicIdGenerator peek no change | `proof_atomic_id_generator_peek_no_change` | ✓ |

### drbot-buffer (34 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| ByteBuffer new | `proof_byte_buffer_new` | ✓ |
| ByteBuffer write/read | `proof_byte_buffer_write_read` | ✓ |
| ByteBuffer peek | `proof_byte_buffer_peek` | ✓ |
| ByteBuffer consume | `proof_byte_buffer_consume` | ✓ |
| ByteBuffer clear | `proof_byte_buffer_clear` | ✓ |
| ByteBuffer compact | `proof_byte_buffer_compact` | ✓ |
| ByteBuffer full | `proof_byte_buffer_full` | ✓ |
| DoubleBuffer new | `proof_double_buffer_new` | ✓ |
| DoubleBuffer default | `proof_double_buffer_default` | ✓ |
| DoubleBuffer back_mut | `proof_double_buffer_back_mut` | ✓ |
| DoubleBuffer swap | `proof_double_buffer_swap` | ✓ |
| DoubleBuffer swap twice | `proof_double_buffer_swap_twice` | ✓ |
| LineBuffer new | `proof_line_buffer_new` | ✓ |
| LineBuffer default | `proof_line_buffer_default` | ✓ |
| LineBuffer append | `proof_line_buffer_append` | ✓ |
| LineBuffer next_line no newline | `proof_line_buffer_next_line_no_newline` | ✓ |
| LineBuffer next_line with newline | `proof_line_buffer_next_line_with_newline` | ✓ |
| LineBuffer flush | `proof_line_buffer_flush` | ✓ |
| LineBuffer flush empty | `proof_line_buffer_flush_empty` | ✓ |
| LineBuffer clear | `proof_line_buffer_clear` | ✓ |
| ChunkBuffer new | `proof_chunk_buffer_new` | ✓ |
| ChunkBuffer push no chunk | `proof_chunk_buffer_push_no_chunk` | ✓ |
| ChunkBuffer push creates chunk | `proof_chunk_buffer_push_creates_chunk` | ✓ |
| ChunkBuffer take_chunks | `proof_chunk_buffer_take_chunks` | ✓ |
| ChunkBuffer flush | `proof_chunk_buffer_flush` | ✓ |
| ChunkBuffer flush empty | `proof_chunk_buffer_flush_empty` | ✓ |
| SharedBuffer new | `proof_shared_buffer_new` | ✓ |
| SharedBuffer default | `proof_shared_buffer_default` | ✓ |
| SharedBuffer push/pop | `proof_shared_buffer_push_pop` | ✓ |
| SharedBuffer pop empty | `proof_shared_buffer_pop_empty` | ✓ |
| SharedBuffer capacity limit | `proof_shared_buffer_with_capacity_limit` | ✓ |
| SharedBuffer clear | `proof_shared_buffer_clear` | ✓ |
| SharedBuffer drain | `proof_shared_buffer_drain` | ✓ |
| SharedBuffer clone | `proof_shared_buffer_clone` | ✓ |

### drbot-checked (31 proofs)

| Property | Proof | Status |
|----------|-------|--------|
| Checked new | `proof_checked_new` | ✓ |
| Checked into_inner | `proof_checked_into_inner` | ✓ |
| Checked get_mut | `proof_checked_get_mut` | ✓ |
| Checked default | `proof_checked_default` | ✓ |
| Checked add success | `proof_checked_add_success` | ✓ |
| Checked add overflow | `proof_checked_add_overflow` | ✓ |
| Checked sub success | `proof_checked_sub_success` | ✓ |
| Checked sub underflow | `proof_checked_sub_underflow` | ✓ |
| Checked mul success | `proof_checked_mul_success` | ✓ |
| Checked mul overflow | `proof_checked_mul_overflow` | ✓ |
| Checked div success | `proof_checked_div_success` | ✓ |
| Checked div by zero | `proof_checked_div_by_zero` | ✓ |
| Checked rem success | `proof_checked_rem_success` | ✓ |
| Checked rem by zero | `proof_checked_rem_by_zero` | ✓ |
| Checked pow success | `proof_checked_pow_success` | ✓ |
| Checked pow zero | `proof_checked_pow_zero` | ✓ |
| try_add ok | `proof_try_add_ok` | ✓ |
| try_add err | `proof_try_add_err` | ✓ |
| try_sub ok | `proof_try_sub_ok` | ✓ |
| try_sub err | `proof_try_sub_err` | ✓ |
| try_div ok | `proof_try_div_ok` | ✓ |
| try_div by zero | `proof_try_div_by_zero` | ✓ |
| CheckedExt | `proof_checked_ext` | ✓ |
| OverflowCounter new | `proof_overflow_counter_new` | ✓ |
| OverflowCounter default | `proof_overflow_counter_default` | ✓ |
| OverflowCounter with_value | `proof_overflow_counter_with_value` | ✓ |
| OverflowCounter increment | `proof_overflow_counter_increment` | ✓ |
| OverflowCounter increment max | `proof_overflow_counter_increment_max` | ✓ |
| OverflowCounter decrement | `proof_overflow_counter_decrement` | ✓ |
| OverflowCounter decrement zero | `proof_overflow_counter_decrement_zero` | ✓ |
| OverflowCounter reset | `proof_overflow_counter_reset` | ✓ |

## Writing New Proofs

### Basic Structure

```rust
#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Describe what this proof verifies
    #[kani::proof]
    fn proof_my_property() {
        // Create symbolic inputs
        let x: u32 = kani::any();

        // Add assumptions to constrain inputs
        kani::assume(x < 100);

        // Call the function under test
        let result = my_function(x);

        // Assert the property must hold
        kani::assert(result > 0, "Result must be positive");
    }
}
```

### Common Patterns

**Bounded loops:**
```rust
#[kani::proof]
#[kani::unwind(10)]  // Limit loop iterations
fn proof_bounded_loop() {
    let arr: [u8; 5] = kani::any();
    // ...
}
```

**Floating point:**
```rust
#[kani::proof]
fn proof_float_property() {
    let x: f32 = kani::any();
    kani::assume(x.is_finite());  // Exclude NaN/Inf
    kani::assume(x >= 0.0 && x <= 1.0);  // Bound range
    // ...
}
```

**Enums:**
```rust
#[kani::proof]
fn proof_enum_property() {
    let val: u8 = kani::any();
    kani::assume(val <= 2);
    let state = match val {
        0 => State::A,
        1 => State::B,
        _ => State::C,
    };
    // ...
}
```

## CI Integration

Add to your CI workflow:

```yaml
kani-verification:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Setup Kani
      uses: model-checking/kani-github-action@v1
    - name: Run Kani proofs
      run: cargo kani --package drbot-kani
```

## Limitations

- Kani doesn't support all Rust features (e.g., inline assembly, some std APIs)
- Async functions need special handling
- Large state spaces may timeout
- Not a replacement for testing—use both

## Resources

- [Kani Documentation](https://model-checking.github.io/kani/)
- [Kani Tutorial](https://model-checking.github.io/kani/kani-tutorial.html)
- [Writing Proofs Guide](https://model-checking.github.io/kani/tutorial-first-steps.html)
