#[derive(Debug, Clone, Copy, Default)]
pub struct DerivativePair {
    pub first: f64,
    pub second: f64,
}

pub fn smooth(values: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 || values.len() < 2 {
        return values.to_vec();
    }

    let mut out = Vec::with_capacity(values.len());
    let mut sum = 0.0;
    for (i, &value) in values.iter().enumerate() {
        sum += value;
        if i >= window {
            sum -= values[i - window];
        }
        let count = (i + 1).min(window) as f64;
        out.push(sum / count);
    }
    out
}

pub fn moving_average(values: &[f64], window: usize) -> Vec<Option<f64>> {
    let window = window.max(1);
    let mut out = vec![None; values.len()];
    let mut sum = 0.0;
    for (i, &value) in values.iter().enumerate() {
        sum += value;
        if i >= window {
            sum -= values[i - window];
        }
        if i + 1 >= window {
            out[i] = Some(sum / window as f64);
        }
    }
    out
}

pub fn derivatives(x: &[f64], y: &[f64]) -> Result<Vec<DerivativePair>, String> {
    if x.len() != y.len() {
        return Err("时间轴与价格序列长度不一致".to_string());
    }

    let n = x.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    for i in 0..n {
        if !x[i].is_finite() || !y[i].is_finite() {
            return Err("数据包含非有限值".to_string());
        }
        if i > 0 && x[i] <= x[i - 1] {
            return Err("时间轴必须严格递增".to_string());
        }
    }

    if n == 1 {
        return Ok(vec![DerivativePair::default()]);
    }

    if n == 2 {
        let slope = (y[1] - y[0]) / (x[1] - x[0]);
        return Ok(vec![
            DerivativePair {
                first: slope,
                second: 0.0,
            },
            DerivativePair {
                first: slope,
                second: 0.0,
            },
        ]);
    }

    let mut out = vec![DerivativePair::default(); n];

    for i in 1..n - 1 {
        let h1 = x[i] - x[i - 1];
        let h2 = x[i + 1] - x[i];
        let y0 = y[i - 1];
        let y1 = y[i];
        let y2 = y[i + 1];

        out[i].first =
            -h2 * y0 / (h1 * (h1 + h2)) + (h2 - h1) * y1 / (h1 * h2) + h1 * y2 / (h2 * (h1 + h2));
        out[i].second =
            2.0 * y0 / (h1 * (h1 + h2)) - 2.0 * y1 / (h1 * h2) + 2.0 * y2 / (h2 * (h1 + h2));
    }

    if n >= 4 {
        let first_weights = finite_difference_weights(&x[..4], x[0], 1);
        let second_weights = finite_difference_weights(&x[..4], x[0], 2);
        out[0].first = dot(&first_weights, &y[..4]);
        out[0].second = dot(&second_weights, &y[..4]);

        let start = n - 4;
        let second_weights = finite_difference_weights(&x[start..], x[n - 1], 2);
        out[n - 1].first = (y[n - 1] - y[n - 2]) / (x[n - 1] - x[n - 2]);
        out[n - 1].second = dot(&second_weights, &y[start..]);
    } else {
        let h1 = x[1] - x[0];
        let h2 = x[2] - x[1];
        let y0 = y[0];
        let y1 = y[1];
        let y2 = y[2];

        out[0].first = -(2.0 * h1 + h2) * y0 / (h1 * (h1 + h2)) + (h1 + h2) * y1 / (h1 * h2)
            - h1 * y2 / (h2 * (h1 + h2));
        out[0].second =
            2.0 * y0 / (h1 * (h1 + h2)) - 2.0 * y1 / (h1 * h2) + 2.0 * y2 / (h2 * (h1 + h2));

        let i = n - 1;
        let h1 = x[i] - x[i - 1];
        let h2 = x[i - 1] - x[i - 2];
        let yl = y[i];
        let ym = y[i - 1];
        let yb = y[i - 2];

        out[i].first = (yl - ym) / h1;
        out[i].second =
            2.0 * yl / (h1 * (h1 + h2)) - 2.0 * ym / (h1 * h2) + 2.0 * yb / (h2 * (h1 + h2));
    }

    Ok(out)
}

pub fn derivatives_sparse(
    x: &[f64],
    y: &[Option<f64>],
    smooth_window: usize,
) -> Result<Vec<Option<DerivativePair>>, String> {
    if x.len() != y.len() {
        return Err("时间轴与价格序列长度不一致".to_string());
    }

    let indices: Vec<usize> = y
        .iter()
        .enumerate()
        .filter_map(|(index, value)| match value {
            Some(value) if value.is_finite() => Some(index),
            _ => None,
        })
        .collect();
    let mut out = vec![None; y.len()];
    if indices.is_empty() {
        return Ok(out);
    }

    let compact_x: Vec<f64> = indices.iter().map(|&index| x[index]).collect();
    let compact_y: Vec<f64> = indices.iter().map(|&index| y[index].unwrap()).collect();
    let smoothed = smooth(&compact_y, smooth_window);
    let pairs = derivatives(&compact_x, &smoothed)?;
    for (index, pair) in indices.into_iter().zip(pairs) {
        out[index] = Some(pair);
    }
    Ok(out)
}

fn dot(weights: &[f64], values: &[f64]) -> f64 {
    weights.iter().zip(values).map(|(w, v)| w * v).sum()
}

fn finite_difference_weights(xs: &[f64], x0: f64, order: usize) -> Vec<f64> {
    let shifted: Vec<f64> = xs.iter().map(|value| value - x0).collect();
    let xs = shifted.as_slice();
    let x0: f64 = 0.0;
    let n = xs.len();
    let mut weights = vec![0.0; n];

    for (j, &xj) in xs.iter().enumerate() {
        let mut denominator = 1.0;
        for (k, &xk) in xs.iter().enumerate() {
            if k != j {
                denominator *= xj - xk;
            }
        }

        let mut coefficients = vec![1.0];
        for (k, &xk) in xs.iter().enumerate() {
            if k == j {
                continue;
            }
            let mut next = vec![0.0; coefficients.len() + 1];
            for (power, coefficient) in coefficients.iter().enumerate() {
                next[power] -= xk * coefficient;
                next[power + 1] += coefficient;
            }
            coefficients = next;
        }

        let derivative = match order {
            1 => coefficients
                .iter()
                .enumerate()
                .skip(1)
                .map(|(power, coefficient)| power as f64 * coefficient * x0.powi(power as i32 - 1))
                .sum::<f64>(),
            2 => coefficients
                .iter()
                .enumerate()
                .skip(2)
                .map(|(power, coefficient)| {
                    (power * (power - 1)) as f64 * coefficient * x0.powi(power as i32 - 2)
                })
                .sum::<f64>(),
            _ => 0.0,
        };

        weights[j] = derivative / denominator;
    }

    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "actual={actual}, expected={expected}"
        );
    }

    #[test]
    fn linear_uniform() {
        let x: Vec<f64> = (0..6).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 3.0 * v + 5.0).collect();
        let result = derivatives(&x, &y).unwrap();
        for pair in &result {
            assert_close(pair.first, 3.0);
            assert_close(pair.second, 0.0);
        }
    }

    #[test]
    fn quadratic_uniform() {
        let x: Vec<f64> = (0..6).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| v * v).collect();
        let result = derivatives(&x, &y).unwrap();
        for (i, pair) in result.iter().enumerate() {
            let expected_first = if i == result.len() - 1 {
                (y[i] - y[i - 1]) / (x[i] - x[i - 1])
            } else {
                2.0 * i as f64
            };
            assert_close(pair.first, expected_first);
            assert_close(pair.second, 2.0);
        }
    }

    #[test]
    fn quadratic_non_uniform() {
        let x = vec![0.0, 1.0, 3.0, 6.0, 10.0, 15.0];
        let y: Vec<f64> = x.iter().map(|v| v * v).collect();
        let result = derivatives(&x, &y).unwrap();
        for (i, pair) in result.iter().enumerate() {
            let expected_first = if i == result.len() - 1 {
                (y[i] - y[i - 1]) / (x[i] - x[i - 1])
            } else {
                2.0 * x[i]
            };
            assert_close(pair.first, expected_first);
            assert_close(pair.second, 2.0);
        }
    }

    #[test]
    fn finite_difference_weights_match_known_values() {
        let xs = [0.0, 1.0, 2.0, 3.0];
        let first = finite_difference_weights(&xs, 0.0, 1);
        for (actual, expected) in first.iter().zip([-11.0 / 6.0, 3.0, -3.0 / 2.0, 1.0 / 3.0]) {
            assert_close(*actual, expected);
        }

        let second = finite_difference_weights(&xs, 0.0, 2);
        for (actual, expected) in second.iter().zip([2.0, -5.0, 4.0, -1.0]) {
            assert_close(*actual, expected);
        }

        let offset = 30_000_000.0;
        let xs = [offset, offset + 1.0, offset + 2.0, offset + 3.0];
        let first = finite_difference_weights(&xs, offset, 1);
        for (actual, expected) in first.iter().zip([-11.0 / 6.0, 3.0, -3.0 / 2.0, 1.0 / 3.0]) {
            assert_close(*actual, expected);
        }
    }

    #[test]
    fn cubic_non_uniform_endpoints() {
        let x = vec![0.0, 1.0, 3.0, 6.0, 10.0, 15.0];
        let y: Vec<f64> = x
            .iter()
            .map(|value| {
                let v = *value;
                v * v * v - 2.0 * v * v + 3.0 * v + 1.0
            })
            .collect();
        let result = derivatives(&x, &y).unwrap();

        for i in [0, x.len() - 1] {
            let expected_first = if i == 0 {
                3.0 * x[i].powi(2) - 4.0 * x[i] + 3.0
            } else {
                (y[i] - y[i - 1]) / (x[i] - x[i - 1])
            };
            assert_close(result[i].first, expected_first);
            assert_close(result[i].second, 6.0 * x[i] - 4.0);
        }
    }

    #[test]
    fn two_points() {
        let x = vec![0.0, 2.0];
        let y = vec![1.0, 5.0];
        let result = derivatives(&x, &y).unwrap();
        assert_eq!(result.len(), 2);
        assert_close(result[0].first, 2.0);
        assert_close(result[1].first, 2.0);
        assert_close(result[0].second, 0.0);
    }

    #[test]
    fn smoothing_window() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        let result = smooth(&values, 2);
        let expected = [1.0, 1.5, 2.5, 3.5, 4.5];
        for (actual, expected) in result.iter().zip(expected) {
            assert_close(*actual, expected);
        }
    }

    #[test]
    fn smoothing_disabled() {
        let values = [3.0, 1.0, 4.0];
        assert_eq!(smooth(&values, 1), values);
        assert_eq!(smooth(&values, 0), values);
    }

    #[test]
    fn rejects_unsorted_time_axis() {
        let x = vec![0.0, 2.0, 1.0];
        let y = vec![1.0, 2.0, 3.0];
        assert!(derivatives(&x, &y).is_err());
    }

    #[test]
    fn rejects_mismatched_lengths() {
        assert!(derivatives(&[0.0, 1.0], &[1.0]).is_err());
    }

    #[test]
    fn moving_average_starts_when_window_filled() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        let result = moving_average(&values, 3);
        assert_eq!(result[0], None);
        assert_eq!(result[1], None);
        assert_close(result[2].unwrap(), 2.0);
        assert_close(result[3].unwrap(), 3.0);
        assert_close(result[4].unwrap(), 4.0);
    }

    #[test]
    fn sparse_derivatives_preserve_offsets() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [None, None, Some(4.0), Some(9.0), Some(16.0)];
        let result = derivatives_sparse(&x, &y, 1).unwrap();
        assert!(result[0].is_none());
        assert!(result[1].is_none());
        assert_close(result[2].unwrap().first, 4.0);
        assert_close(result[2].unwrap().second, 2.0);
        assert_close(result[4].unwrap().first, 7.0);
    }
}
