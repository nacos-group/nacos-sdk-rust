use rand::Rng;

use crate::api::error::Error::NoAvailableServiceInstance;
use crate::api::error::Error::WeightCalculateFailed;
use crate::api::error::Result;
use crate::api::naming::{InstanceChooser, ServiceInstance};

pub(crate) struct RandomWeightChooser {
    weights: Vec<f64>,

    items: Vec<ServiceInstance>,
}

impl RandomWeightChooser {
    pub fn new(service_name: String, items: Vec<ServiceInstance>) -> Result<Self> {
        if items.is_empty() {
            return Err(NoAvailableServiceInstance(service_name));
        }
        let mut init_items: Vec<ServiceInstance> = Vec::with_capacity(items.len());
        let mut origin_weight_sum = 0_f64;

        let mut count = 0;
        for instance in items.iter() {
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
            count += 1;
        }

        let mut exact_weights: Vec<f64> = Vec::with_capacity(count);
        let mut index = 0;

        for instance in items.into_iter() {
            let single_weight = instance.weight;

            if single_weight <= 0_f64 {
                continue;
            }

            init_items.push(instance);

            exact_weights.insert(index, single_weight / origin_weight_sum);
            index += 1;
        }

        let mut weights: Vec<f64> = Vec::with_capacity(count);

        let mut random_range = 0_f64;

        for (i, exact_weights_item) in exact_weights.iter().enumerate().take(index) {
            weights.insert(i, random_range + exact_weights_item);
            random_range += exact_weights_item;
        }

        let double_precision_delta = 0.0001_f64;

        if index == 0 || (weights[index - 1] - 1.0_f64).abs() < double_precision_delta {
            return Ok(RandomWeightChooser {
                weights,
                items: init_items,
            });
        }

        Err(WeightCalculateFailed)
    }
}

impl InstanceChooser for RandomWeightChooser {
    fn choose(mut self) -> Option<ServiceInstance> {
        let mut rng = rand::thread_rng();
        let random_number = rng.gen_range(0.0..1.0);
        let index = self
            .weights
            .binary_search_by(|d| d.partial_cmp(&random_number).unwrap());
        if let Ok(index) = index {
            let instance = self.items.get(index);
            if let Some(instance) = instance {
                return Some(instance.to_owned());
            } else {
                return self.items.pop();
            }
        } else {
            let index = index.unwrap_err();
            if index < self.weights.len() {
                let weight = self.weights.get(index);
                if weight.is_none() {
                    return self.items.pop();
                }
                let weight = weight.unwrap().to_owned();
                if random_number < weight {
                    let instance = self.items.get(index);
                    if instance.is_none() {
                        return self.items.pop();
                    }
                    let instance = instance.unwrap();
                    return Some(instance.to_owned());
                }
            }
        }

        self.items.pop()
    }
}
