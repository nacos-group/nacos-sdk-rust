use std::cmp::Ordering;

use crate::api::error::Error::ErrResult;
use crate::api::error::Result;
use crate::api::naming::{InstanceChooser, ServiceInstance};
use rand::RngExt;

pub(crate) struct RandomWeightChooser<'a> {
    weights: Vec<f64>,
    indices: Vec<usize>,
    items: &'a [ServiceInstance],
}

impl<'a> RandomWeightChooser<'a> {
    pub fn new(
        service_name: &str,
        items: &'a [ServiceInstance],
        keep: impl Fn(&ServiceInstance) -> bool,
    ) -> Result<Self> {
        let mut indices = Vec::new();
        let mut origin_weight_sum = 0_f64;

        for (index, instance) in items.iter().enumerate() {
            if !keep(instance) {
                continue;
            }

            let mut weight = instance.weight;

            if weight <= 0_f64 {
                continue;
            }

            if weight.is_infinite() {
                weight = 10000.0_f64;
            }

            if weight.is_nan() {
                weight = 1.0_f64;
            }
            origin_weight_sum += weight;
            indices.push(index);
        }

        if indices.is_empty() {
            return Err(ErrResult(format!(
                "no available {service_name} service instance can be selected"
            )));
        }

        let mut weights = Vec::with_capacity(indices.len());
        let mut random_range = 0_f64;
        for &index in indices.iter() {
            random_range += items[index].weight / origin_weight_sum;
            weights.push(random_range);
        }

        let double_precision_delta = 0.0001_f64;

        if (weights[weights.len() - 1] - 1.0_f64).abs() < double_precision_delta {
            Ok(Self {
                weights,
                indices,
                items,
            })
        } else {
            Err(ErrResult(
                "Cumulative Weight calculate wrong , the sum of probabilities does not equals 1."
                    .to_string(),
            ))
        }
    }

    pub(crate) fn choose_index(&self) -> usize {
        let random_number = rand::rng().random_range(0.0..1.0);
        let position = match self
            .weights
            .binary_search_by(|weight| weight.partial_cmp(&random_number).unwrap_or(Ordering::Less))
        {
            Ok(index) => index,
            Err(index) if index < self.weights.len() => {
                if random_number < self.weights[index] {
                    index
                } else {
                    self.weights.len() - 1
                }
            }
            Err(_) => self.weights.len() - 1,
        };
        self.indices[position]
    }
}

impl InstanceChooser for RandomWeightChooser<'_> {
    fn choose(self) -> Option<ServiceInstance> {
        let index = self.choose_index();
        Some(self.items[index].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::RandomWeightChooser;
    use crate::api::naming::ServiceInstance;

    fn instance(ip: &str, weight: f64, healthy: bool) -> ServiceInstance {
        ServiceInstance {
            ip: ip.to_string(),
            weight,
            healthy,
            ..Default::default()
        }
    }

    #[test]
    fn test_choose_index_only_returns_eligible_instances() {
        let instances = vec![
            instance("a", 0.0, true),
            instance("b", 1.0, false),
            instance("c", 1.0, true),
            instance("d", 2.0, true),
        ];

        let chooser =
            RandomWeightChooser::new("svc", &instances, |item| item.healthy && item.weight > 0.0)
                .expect("chooser should build");

        for _ in 0..100 {
            let index = chooser.choose_index();
            assert!(index == 2 || index == 3, "unexpected index {index}");
        }
    }

    #[test]
    fn test_weight_distribution() {
        let instances = vec![instance("a", 1.0, true), instance("b", 3.0, true)];
        let chooser =
            RandomWeightChooser::new("svc", &instances, |_| true).expect("chooser should build");

        let draws = 10_000;
        let second = (0..draws).filter(|_| chooser.choose_index() == 1).count();
        let ratio = second as f64 / draws as f64;
        assert!((ratio - 0.75).abs() < 0.05, "unexpected ratio {ratio}");
    }

    #[test]
    fn test_non_finite_weights_are_rejected() {
        let infinite = vec![instance("a", f64::INFINITY, true), instance("b", 1.0, true)];
        assert!(RandomWeightChooser::new("svc", &infinite, |_| true).is_err());

        let nan = vec![instance("a", f64::NAN, true), instance("b", 1.0, true)];
        assert!(RandomWeightChooser::new("svc", &nan, |_| true).is_err());
    }

    #[test]
    fn test_empty_candidates_error() {
        let instances = vec![instance("a", 0.0, true)];
        assert!(RandomWeightChooser::new("svc", &instances, |item| item.weight > 0.0).is_err());
    }
}
