use rand::random;
use rayon::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::genome::{crossover, Genome, GenomeId};
use crate::mutations::MutationKind;
use crate::network::Network;
use crate::speciation::SpeciesSet;
pub use configuration::Configuration;
use reporter::Reporter;
use speciation::GenomeBank;

mod configuration;
mod reporter;
mod speciation;

pub struct NEAT {
    inputs: usize,
    outputs: usize,
    fitness_fn: fn(&mut Network) -> f64,
    pub genomes: GenomeBank,
    species: SpeciesSet,
    configuration: Rc<RefCell<Configuration>>,
    reporter: Reporter,
}

impl NEAT {
    pub fn new(inputs: usize, outputs: usize, fitness_fn: fn(&mut Network) -> f64) -> Self {
        let configuration: Rc<RefCell<Configuration>> = Default::default();

        NEAT {
            inputs,
            outputs,
            fitness_fn,
            genomes: GenomeBank::new(configuration.clone()),
            species: SpeciesSet::new(configuration.clone()),
            configuration,
            reporter: Reporter::new(),
        }
    }

    pub fn set_configuration(&mut self, config: Configuration) {
        *self.configuration.borrow_mut() = config;
    }

    pub fn start(&mut self) -> (Network, f64) {
        let (population_size, max_generations) = {
            let config = self.configuration.borrow();

            (config.population_size, config.max_generations)
        };

        // Create initial genomes
        (0..population_size).for_each(|_| {
            self.genomes
                .add_genome(Genome::new(self.inputs, self.outputs))
        });

        for i in 1..=max_generations {
            self.test_fitness();

            let current_genome_ids: Vec<GenomeId> =
                self.genomes.genomes().keys().cloned().collect();
            let previous_and_current_genomes = self
                .genomes
                .genomes()
                .iter()
                .chain(self.genomes.previous_genomes())
                .map(|(genome_id, genome)| (genome_id.clone(), genome.clone()))
                .collect();

            self.species.speciate(
                i,
                &current_genome_ids,
                &previous_and_current_genomes,
                self.genomes.fitnesses(),
            );

            self.genomes.speciate();

            let elitism = self.configuration.borrow().elitism;
            let population_size = self.configuration.borrow().population_size;
            let mutation_rate = self.configuration.borrow().mutation_rate;
            let survival_ratio = self.configuration.borrow().survival_ratio;

            let survived_count = (self.genomes.genomes().len()
                * (survival_ratio * 100.).round() as usize)
                .div_euclid(100);

            let elites_count =
                (self.genomes.genomes().len() * (elitism * 100.).round() as usize).div_euclid(100);

            let all_genomes: Vec<&Genome> = self
                .genomes_by_adjusted_fitness()
                .iter()
                .take(survived_count)
                .map(|(genome, _)| *genome)
                .collect();
            let mut elites: Vec<Genome> = all_genomes
                .iter()
                .take(elites_count)
                .cloned()
                .cloned()
                .collect();
            let non_elites = all_genomes;

            let mut offspring = vec![];

            while (elites.len() + offspring.len()) < population_size {
                let crossover_data: Vec<(&Genome, f64, &Genome, f64)> = (0..population_size
                    - (elites.len() + offspring.len()))
                    .map(|_| {
                        let parent_index_a = random::<usize>() % non_elites.len();
                        let parent_a = non_elites.get(parent_index_a).unwrap();

                        let parent_fitness_a =
                            self.genomes.fitnesses().get(&parent_a.id()).unwrap();

                        let parent_index_b = random::<usize>() % non_elites.len();
                        let parent_b = non_elites.get(parent_index_b).unwrap();
                        let parent_fitness_b =
                            self.genomes.fitnesses().get(&parent_b.id()).unwrap();

                        (*parent_a, *parent_fitness_a, *parent_b, *parent_fitness_b)
                    })
                    .collect();

                let mut children: Vec<Genome> = crossover_data
                    .par_iter()
                    .map(|(parent_a, fitness_a, parent_b, fitness_b)| {
                        crossover((parent_a, *fitness_a), (parent_b, *fitness_b))
                    })
                    .filter(|maybe_genome| maybe_genome.is_some())
                    .map(|maybe_genome| maybe_genome.unwrap())
                    .collect();

                let mutations_for_children: Vec<Option<MutationKind>> = children
                    .iter()
                    .map(|_| {
                        if random::<f64>() < mutation_rate {
                            Some(self.pick_mutation())
                        } else {
                            None
                        }
                    })
                    .collect();

                children
                    .par_iter_mut()
                    .zip(mutations_for_children)
                    .for_each(|(child, maybe_mutation)| {
                        if let Some(mutation) = maybe_mutation {
                            child.mutate(&mutation);
                        }
                    });

                offspring.append(&mut children);
            }

            let mut new_genomes = vec![];
            new_genomes.append(&mut elites);
            new_genomes.append(&mut offspring);

            self.genomes.clear();
            new_genomes
                .into_iter()
                .for_each(|genome| self.genomes.add_genome(genome));

            self.test_fitness();

            self.reporter.report(i, &self);

            let goal_reached = {
                if let Some(goal) = self.configuration.borrow().fitness_goal {
                    let (_, _, best_fitness) = self.get_best();

                    best_fitness >= goal
                } else {
                    false
                }
            };

            if goal_reached {
                break;
            }
        }

        let (_, best_genome, best_fitness) = self.get_best();
        (Network::from(best_genome), best_fitness)
    }

    fn genomes_by_adjusted_fitness(&self) -> Vec<(&Genome, f64)> {
        let mut genomes: Vec<(&u64, &Genome)> = self.genomes.genomes().iter().collect();
        let adjusted_fitnesses = self.genomes.adjusted_fitnesses();

        genomes.sort_by(|a, b| {
            let fitness_a = adjusted_fitnesses.get(a.0).unwrap();
            let fitness_b = adjusted_fitnesses.get(b.0).unwrap();

            if (fitness_a - fitness_b).abs() < f64::EPSILON {
                std::cmp::Ordering::Equal
            } else if fitness_a > fitness_b {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        genomes
            .into_iter()
            .map(|(index, genome)| (genome, *adjusted_fitnesses.get(index).unwrap()))
            .collect()
    }

    fn test_fitness(&mut self) {
        let ids_and_networks: Vec<(u64, Network)> = self
            .genomes
            .genomes()
            .iter()
            .map(|(genome_id, genome)| (*genome_id, Network::from(genome)))
            .collect();

        let node_cost = self.configuration.borrow().node_cost;
        let connection_cost = self.configuration.borrow().connection_cost;
        let fitness_fn = self.fitness_fn;

        let ids_and_fitnesses: Vec<(u64, f64)> = ids_and_networks
            .into_par_iter()
            .map(|(genome_id, mut network)| {
                let mut fitness: f64 = (fitness_fn)(&mut network);
                fitness -= node_cost * network.nodes.len() as f64;
                fitness -= connection_cost * network.connections.len() as f64;

                (genome_id, fitness)
            })
            .collect();

        ids_and_fitnesses
            .into_iter()
            .for_each(|(genome_id, genome_fitness)| {
                self.genomes.mark_fitness(genome_id, genome_fitness)
            });
    }

    pub fn get_best(&self) -> (GenomeId, &Genome, f64) {
        let (best_genome_id, best_fitness) = self.genomes.fitnesses().iter().fold(
            (0, 0.),
            |(best_id, best_fitness), (genome_id, genome_fitness)| {
                if *genome_fitness > best_fitness {
                    (*genome_id, *genome_fitness)
                } else {
                    (best_id, best_fitness)
                }
            },
        );

        let best_genome = self.genomes.genomes().get(&best_genome_id).unwrap();

        (best_genome_id, best_genome, best_fitness)
    }

    fn pick_mutation(&self) -> MutationKind {
        use rand::{distributions::Distribution, thread_rng};
        use rand_distr::weighted_alias::WeightedAliasIndex;

        let dist = WeightedAliasIndex::new(
            self.configuration
                .borrow()
                .mutation_kinds
                .iter()
                .map(|k| k.1)
                .collect(),
        )
        .unwrap();

        let mut rng = thread_rng();

        self.configuration
            .borrow()
            .mutation_kinds
            .get(dist.sample(&mut rng))
            .cloned()
            .unwrap()
            .0
    }

    pub fn add_hook(&mut self, every: usize, hook: reporter::Hook) {
        self.reporter.register(every, hook);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor() {
        let mut system = NEAT::new(2, 1, |n| {
            let inputs: Vec<Vec<f64>> =
                vec![vec![0., 0.], vec![0., 1.], vec![1., 0.], vec![1., 1.]];
            let outputs: Vec<f64> = vec![0., 1., 1., 0.];

            let mut error = 0.;

            for (i, o) in inputs.iter().zip(outputs) {
                let results = n.forward_pass(i.clone());
                let result = results.first().unwrap();

                error += (o - *result).powi(2);
            }

            1. / (1. + error)
        });

        system.set_configuration(Configuration {
            population_size: 25,
            max_generations: 3,
            mutation_rate: 0.75,
            fitness_goal: Some(0.95),
            node_cost: 0.,
            connection_cost: 0.,
            compatibility_threshold: 3.,
            ..Default::default()
        });
        system.add_hook(1, |i, system| {
            // let (_, _, fitness) = system.get_best();
            // println!("Generation {}, best fitness is {}", i, fitness);
        });

        let (mut network, fitness) = system.start();

        // let inputs: Vec<Vec<f64>> = vec![vec![0., 0.], vec![0., 1.], vec![1., 0.], vec![1., 1.]];
        // for i in inputs {
        //     let o = network.forward_pass(i.clone());
        //     dbg!(i, o);
        // }

        // dbg!(&network, &fitness);

        println!(
            "Found network with {} nodes and {} connections, of fitness {}",
            network.nodes.len(),
            network.connections.len(),
            fitness
        );
    }
}
