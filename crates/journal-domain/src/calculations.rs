use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::models::{AverageMode, AveragePrecision, GradeEntry};

/// Minimal numeric grade sample for pure calculation & forecasting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradeSample {
    pub value: Decimal,
    pub weight: Decimal,
}

impl GradeSample {
    pub fn new(value: Decimal, weight: Decimal) -> Self {
        Self { value, weight }
    }
}

/// Extracts only numeric entries that actively participate in average calculation.
/// Non-numeric marks (Н/Б, Уваж., Зач., Н/З, custom marks with `counts_in_average = false`) are excluded.
pub fn extract_countable_samples(entries: &[GradeEntry]) -> Vec<GradeSample> {
    entries
        .iter()
        .filter(|e| e.counts_in_average)
        .filter_map(|e| {
            e.numeric_value.map(|v| GradeSample {
                value: v,
                weight: e.weight,
            })
        })
        .collect()
}

/// Calculates exact arithmetic or weighted average with zero intermediate rounding.
/// Returns `None` when no countable numeric grades exist (UI renders `—`, never `0`).
pub fn calculate_average_from_samples(samples: &[GradeSample], mode: AverageMode) -> Option<Decimal> {
    if samples.is_empty() {
        return None;
    }

    match mode {
        AverageMode::Arithmetic => {
            let sum: Decimal = samples.iter().map(|s| s.value).sum();
            let count = Decimal::from(samples.len());
            Some(sum / count)
        }
        AverageMode::Weighted => {
            let mut weighted_sum = Decimal::ZERO;
            let mut weight_sum = Decimal::ZERO;
            for s in samples {
                if s.weight > Decimal::ZERO {
                    weighted_sum += s.value * s.weight;
                    weight_sum += s.weight;
                }
            }
            if weight_sum.is_zero() {
                None
            } else {
                Some(weighted_sum / weight_sum)
            }
        }
    }
}

pub fn calculate_average(entries: &[GradeEntry], mode: AverageMode) -> Option<Decimal> {
    let samples = extract_countable_samples(entries);
    calculate_average_from_samples(&samples, mode)
}

/// Rounds strictly at display time using MidpointAwayFromZero to 1, 2, or 3 decimal places.
pub fn format_average(avg: Option<Decimal>, precision: AveragePrecision) -> String {
    match avg {
        None => "—".to_string(),
        Some(val) => {
            let dp = precision.decimals();
            let rounded = val.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero);
            format!("{:.prec$}", rounded, prec = dp as usize)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastResult {
    pub current_average: Option<Decimal>,
    pub projected_average: Option<Decimal>,
    pub delta: Option<Decimal>,
    pub current_grade_count: usize,
    pub added_grade_count: usize,
    pub mode: AverageMode,
    /// Explicitly marks this result as hypothetical (never written to the journal).
    pub is_forecast: bool,
}

/// Projects how adding one or more hypothetical grades affects the average.
pub fn forecast_after_grades(
    current: &[GradeSample],
    hypothetical: &[GradeSample],
    mode: AverageMode,
) -> ForecastResult {
    let current_average = calculate_average_from_samples(current, mode);
    let mut combined = Vec::with_capacity(current.len() + hypothetical.len());
    combined.extend_from_slice(current);
    combined.extend_from_slice(hypothetical);
    let projected_average = calculate_average_from_samples(&combined, mode);
    let delta = match (current_average, projected_average) {
        (Some(c), Some(p)) => Some(p - c),
        _ => None,
    };

    ForecastResult {
        current_average,
        projected_average,
        delta,
        current_grade_count: current.len(),
        added_grade_count: hypothetical.len(),
        mode,
        is_forecast: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetReachability {
    pub current_average: Option<Decimal>,
    pub target_average: Decimal,
    pub assumed_grade: Decimal,
    pub assumed_weight: Decimal,
    pub mode: AverageMode,
    pub already_reached: bool,
    pub reachable: bool,
    pub grades_needed: Option<u32>,
    pub reachable_within_limit: Option<bool>,
    pub max_works_checked: Option<u32>,
    pub projected_average: Option<Decimal>,
    pub is_forecast: bool,
}

/// Computes the exact minimum number of future grades of value `assumed_grade` (with `assumed_weight`)
/// needed to reach or exceed `target_average`, optionally checking feasibility within `max_works`.
pub fn grades_needed_for_target(
    current: &[GradeSample],
    target_average: Decimal,
    assumed_grade: Decimal,
    assumed_weight: Decimal,
    mode: AverageMode,
    max_works: Option<u32>,
) -> TargetReachability {
    let current_average = calculate_average_from_samples(current, mode);
    if let Some(cur) = current_average
        && cur >= target_average
    {
        return TargetReachability {
            current_average,
            target_average,
            assumed_grade,
            assumed_weight,
            mode,
            already_reached: true,
            reachable: true,
            grades_needed: Some(0),
            reachable_within_limit: max_works.map(|_| true),
            max_works_checked: max_works,
            projected_average: current_average,
            is_forecast: true,
        };
    }

    let eff_weight = match mode {
        AverageMode::Arithmetic => Decimal::ONE,
        AverageMode::Weighted => assumed_weight,
    };

    if eff_weight <= Decimal::ZERO {
        return TargetReachability {
            current_average,
            target_average,
            assumed_grade,
            assumed_weight,
            mode,
            already_reached: false,
            reachable: false,
            grades_needed: None,
            reachable_within_limit: max_works.map(|_| false),
            max_works_checked: max_works,
            projected_average: current_average,
            is_forecast: true,
        };
    }

    let (sum_v, sum_w): (Decimal, Decimal) = match mode {
        AverageMode::Arithmetic => (
            current.iter().map(|s| s.value).sum(),
            Decimal::from(current.len()),
        ),
        AverageMode::Weighted => current
            .iter()
            .filter(|s| s.weight > Decimal::ZERO)
            .fold((Decimal::ZERO, Decimal::ZERO), |(sv, sw), s| {
                (sv + s.value * s.weight, sw + s.weight)
            }),
    };

    // If no grades yet, 1 grade suffices iff assumed_grade >= target_average
    if sum_w.is_zero() {
        let ok = assumed_grade >= target_average;
        let needed = ok.then_some(1);
        return TargetReachability {
            current_average: None,
            target_average,
            assumed_grade,
            assumed_weight,
            mode,
            already_reached: false,
            reachable: ok,
            grades_needed: needed,
            reachable_within_limit: max_works.map(|lim| ok && lim >= 1),
            max_works_checked: max_works,
            projected_average: ok.then_some(assumed_grade),
            is_forecast: true,
        };
    }

    // Need: (sum_v + n * assumed_grade * eff_weight) / (sum_w + n * eff_weight) >= target
    // <=> n * eff_weight * (assumed_grade - target) >= target * sum_w - sum_v
    let deficit = target_average * sum_w - sum_v;
    let gain_per_grade = eff_weight * (assumed_grade - target_average);

    if gain_per_grade <= Decimal::ZERO {
        // Cannot strictly increase toward target when assumed_grade <= target (and cur < target)
        return TargetReachability {
            current_average,
            target_average,
            assumed_grade,
            assumed_weight,
            mode,
            already_reached: false,
            reachable: false,
            grades_needed: None,
            reachable_within_limit: max_works.map(|_| false),
            max_works_checked: max_works,
            projected_average: current_average,
            is_forecast: true,
        };
    }

    let exact_n = (deficit / gain_per_grade).ceil();
    let mut n: u32 = match exact_n.try_into() {
        Ok(v) if v <= 10_000 => v,
        _ => {
            return TargetReachability {
                current_average,
                target_average,
                assumed_grade,
                assumed_weight,
                mode,
                already_reached: false,
                reachable: false,
                grades_needed: None,
                reachable_within_limit: max_works.map(|_| false),
                max_works_checked: max_works,
                projected_average: current_average,
                is_forecast: true,
            };
        }
    };

    // Verify exact inequality against division rounding edge cases
    while n > 0 {
        let n_dec = Decimal::from(n);
        let new_avg = (sum_v + n_dec * assumed_grade * eff_weight) / (sum_w + n_dec * eff_weight);
        if new_avg >= target_average {
            break;
        }
        n += 1;
    }

    let n_dec = Decimal::from(n);
    let projected = (sum_v + n_dec * assumed_grade * eff_weight) / (sum_w + n_dec * eff_weight);

    TargetReachability {
        current_average,
        target_average,
        assumed_grade,
        assumed_weight,
        mode,
        already_reached: false,
        reachable: true,
        grades_needed: Some(n),
        reachable_within_limit: max_works.map(|lim| n <= lim),
        max_works_checked: max_works,
        projected_average: Some(projected),
        is_forecast: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodMetricDelta {
    pub baseline_average: Option<Decimal>,
    pub comparison_average: Option<Decimal>,
    pub absolute_delta: Option<Decimal>,
    /// Percentage delta is `Some` only when baseline is non-zero and meaningful.
    pub percent_delta: Option<Decimal>,
    pub baseline_grade_count: usize,
    pub comparison_grade_count: usize,
    pub grade_count_delta: i64,
}

pub fn compare_period_metrics(
    baseline: &[GradeSample],
    comparison: &[GradeSample],
    mode: AverageMode,
) -> PeriodMetricDelta {
    let base_avg = calculate_average_from_samples(baseline, mode);
    let comp_avg = calculate_average_from_samples(comparison, mode);
    let absolute_delta = match (base_avg, comp_avg) {
        (Some(b), Some(c)) => Some(c - b),
        _ => None,
    };
    let percent_delta = match (base_avg, absolute_delta) {
        (Some(b), Some(d)) if !b.is_zero() => Some((d / b) * Decimal::from(100)),
        _ => None,
    };
    PeriodMetricDelta {
        baseline_average: base_avg,
        comparison_average: comp_avg,
        absolute_delta,
        percent_delta,
        baseline_grade_count: baseline.len(),
        comparison_grade_count: comparison.len(),
        grade_count_delta: comparison.len() as i64 - baseline.len() as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MarkCategory, RecordSource, WorkType, builtin_marks};
    use chrono::{NaiveDate, Utc};
    use proptest::prelude::*;
    use uuid::Uuid;

    fn sample_entry(val: Option<i64>, weight: i64, counts: bool) -> GradeEntry {
        let now = Utc::now();
        GradeEntry {
            id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            subject_id: Uuid::new_v4(),
            date: NaiveDate::from_ymd_opt(2026, 11, 26).unwrap(),
            time: None,
            mark_type_id: builtin_marks::GRADE_5_ID,
            mark_code: "grade_5".into(),
            mark_label: "5".into(),
            mark_category: if counts {
                MarkCategory::NumericGrade
            } else {
                MarkCategory::PassFail
            },
            counts_in_average: counts,
            numeric_value: val.map(Decimal::from),
            weight: Decimal::from(weight),
            work_type: WorkType::ClassAnswer,
            topic: String::new(),
            comment: String::new(),
            teacher: String::new(),
            source: RecordSource::Gui,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn empty_and_non_counting_marks_return_none() {
        assert_eq!(calculate_average(&[], AverageMode::Arithmetic), None);
        assert_eq!(format_average(None, AveragePrecision::Two), "—");

        let only_non_numeric = vec![
            sample_entry(None, 1, false),
            sample_entry(Some(5), 1, false), // custom mark excluded by counts_in_average=false
        ];
        assert_eq!(
            calculate_average(&only_non_numeric, AverageMode::Arithmetic),
            None
        );
    }

    #[test]
    fn weighted_vs_arithmetic_average_exactness() {
        // 4 (w=1), 5 (w=2), 3 (w=3) => arithmetic = 12/3 = 4.00; weighted = (4 + 10 + 9)/6 = 23/6 = 3.8333...
        let entries = vec![
            sample_entry(Some(4), 1, true),
            sample_entry(Some(5), 2, true),
            sample_entry(Some(3), 3, true),
            sample_entry(None, 1, false), // Pass/Fail must not affect average
        ];
        let arith = calculate_average(&entries, AverageMode::Arithmetic);
        let weighted = calculate_average(&entries, AverageMode::Weighted);

        assert_eq!(format_average(arith, AveragePrecision::Two), "4.00");
        assert_eq!(format_average(weighted, AveragePrecision::One), "3.8");
        assert_eq!(format_average(weighted, AveragePrecision::Two), "3.83");
        assert_eq!(format_average(weighted, AveragePrecision::Three), "3.833");
    }

    #[test]
    fn grades_needed_for_target_handles_reachable_and_unreachable() {
        // Current: [4, 4] -> avg 4.0. Need 4.5 using 5s -> need 2 fives => (8+10)/4 = 4.5
        let current = vec![
            GradeSample::new(Decimal::from(4), Decimal::ONE),
            GradeSample::new(Decimal::from(4), Decimal::ONE),
        ];
        let res = grades_needed_for_target(
            &current,
            Decimal::new(45, 1),
            Decimal::from(5),
            Decimal::ONE,
            AverageMode::Arithmetic,
            Some(3),
        );
        assert!(res.reachable);
        assert_eq!(res.grades_needed, Some(2));
        assert_eq!(res.reachable_within_limit, Some(true));

        // Target 5.0 is unreachable when current has 4s
        let unreachable = grades_needed_for_target(
            &current,
            Decimal::from(5),
            Decimal::from(5),
            Decimal::ONE,
            AverageMode::Arithmetic,
            Some(10),
        );
        assert!(!unreachable.reachable);
        assert_eq!(unreachable.grades_needed, None);
        assert_eq!(unreachable.reachable_within_limit, Some(false));

        // Weighted control work (w=2): need 1 five of weight 2 => (4+4+10)/(1+1+2) = 18/4 = 4.5
        let weighted_res = grades_needed_for_target(
            &current,
            Decimal::new(45, 1),
            Decimal::from(5),
            Decimal::from(2),
            AverageMode::Weighted,
            Some(1),
        );
        assert!(weighted_res.reachable);
        assert_eq!(weighted_res.grades_needed, Some(1));
        assert_eq!(weighted_res.reachable_within_limit, Some(true));
    }

    proptest! {
        #[test]
        fn prop_average_always_within_min_and_max(
            grades in prop::collection::vec((1i64..=5i64, 1i64..=5i64), 1..50)
        ) {
            let samples: Vec<GradeSample> = grades
                .iter()
                .map(|(v, w)| GradeSample::new(Decimal::from(*v), Decimal::from(*w)))
                .collect();
            let min_g = Decimal::from(grades.iter().map(|(v, _)| *v).min().unwrap());
            let max_g = Decimal::from(grades.iter().map(|(v, _)| *v).max().unwrap());

            let arith = calculate_average_from_samples(&samples, AverageMode::Arithmetic).unwrap();
            let weighted = calculate_average_from_samples(&samples, AverageMode::Weighted).unwrap();

            prop_assert!(arith >= min_g && arith <= max_g);
            prop_assert!(weighted >= min_g && weighted <= max_g);
        }

        #[test]
        fn prop_weighted_equals_arithmetic_when_weights_equal(
            grades in prop::collection::vec(1i64..=5i64, 1..40),
            w in 1i64..=10i64
        ) {
            let samples: Vec<GradeSample> = grades
                .iter()
                .map(|v| GradeSample::new(Decimal::from(*v), Decimal::from(w)))
                .collect();
            let arith = calculate_average_from_samples(&samples, AverageMode::Arithmetic).unwrap();
            let weighted = calculate_average_from_samples(&samples, AverageMode::Weighted).unwrap();
            prop_assert_eq!(arith.round_dp(12), weighted.round_dp(12));
        }

        #[test]
        fn prop_forecast_needed_grades_actually_reaches_target(
            grades in prop::collection::vec(1i64..=4i64, 1..20),
            target_tenths in 30i64..=49i64,
        ) {
            let samples: Vec<GradeSample> = grades
                .iter()
                .map(|v| GradeSample::new(Decimal::from(*v), Decimal::ONE))
                .collect();
            let target = Decimal::new(target_tenths, 1);
            let reach = grades_needed_for_target(
                &samples,
                target,
                Decimal::from(5),
                Decimal::ONE,
                AverageMode::Arithmetic,
                None,
            );
            if reach.reachable {
                let n = reach.grades_needed.unwrap();
                let mut added = vec![GradeSample::new(Decimal::from(5), Decimal::ONE); n as usize];
                let mut combined = samples.clone();
                combined.append(&mut added);
                let after = calculate_average_from_samples(&combined, AverageMode::Arithmetic).unwrap();
                prop_assert!(after >= target);
                if n > 0 {
                    combined.pop();
                    let one_less = calculate_average_from_samples(&combined, AverageMode::Arithmetic).unwrap();
                    prop_assert!(one_less < target);
                }
            }
        }
    }
}
