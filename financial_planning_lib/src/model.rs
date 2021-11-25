use anyhow::{anyhow, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

use crate::asset::{Category, CategoryName, CategoryValue, Money, Tx};
use crate::flow::{Flow, FlowName};
use crate::tax::{AnnualTaxPolicy, TaxAdjustment, TaxSummary};
use crate::time::{Month, TimeRange, Year};

pub struct Model {
    categories: Vec<Category>,
    flows: BTreeMap<CategoryName, Vec<Flow>>,
    tax_policy: Box<dyn AnnualTaxPolicy>,
    tax_category: CategoryName,
}

#[derive(Debug)]
pub struct ModelReport {
    pub years: BTreeMap<Year, YearlyReport>,
}

#[derive(Debug)]
pub struct YearlyReport {
    pub category_summary: BTreeMap<CategoryName, BTreeMap<Month, MonthlyReport>>,
    pub tax_summary: TaxSummary,
    pub tax_adjustment: TaxAdjustment,
}

#[derive(Debug, Clone)]
pub struct MonthlyReport {
    pub value: Money,
    pub transactions: BTreeMap<FlowName, Tx>,
}

impl Model {
    pub fn new(
        flows: BTreeMap<CategoryName, Vec<Flow>>,
        categories: Vec<Category>,
        tax_policy: Box<dyn AnnualTaxPolicy>,
        tax_category: CategoryName,
    ) -> Result<Self> {
        let out = Self {
            flows,
            categories,
            tax_policy,
            tax_category,
        };
        out.validate().context("Provided inputs were invalid")?;
        Ok(out)
    }

    fn validate(&self) -> Result<()> {
        let valid_cats: BTreeSet<&CategoryName> = self.categories.iter().map(|c| &c.name).collect();
        if !valid_cats.contains(&self.tax_category) {
            return Err(anyhow!(
                "Tax category \"{}\" was not found in provided categories. Options are {:?}",
                self.tax_category.0,
                itertools::join(valid_cats.iter().map(|c| &c.0), ", "),
            ));
        }

        for (cat_name, flows) in &self.flows {
            if !valid_cats.contains(&cat_name) {
                return Err(anyhow!(
                    "Flows ({}) found with unknown cateogry \"{}\". Options are {:?}",
                    itertools::join(flows.iter().map(|f| &f.name.0), ", "),
                    cat_name.0,
                    itertools::join(valid_cats.iter().map(|c| &c.0), ", "),
                ));
            }
        }
        Ok(())
    }

    fn run_year<'year, 'model: 'year>(
        year: Year,
        category_values: &mut Vec<CategoryValue<'model>>,
        flows: &mut BTreeMap<CategoryName, Vec<Flow>>,
        tax_policy: &'year Box<dyn AnnualTaxPolicy>,
        tax_category: &'year CategoryName,
    ) -> YearlyReport {
        let mut summary = BTreeMap::new();
        let mut tax_summary = TaxSummary::new();

        for category_value in category_values.iter_mut() {
            if let Some(flows) = flows.get(&category_value.name()) {
                let mut cat_model = CategoryModel {
                    category_value: category_value,
                    flows,
                };

                let model_output = cat_model.run(year.clone());
                summary.insert(category_value.name().clone(), model_output.clone());

                for (
                    _,
                    MonthlyReport {
                        value: _,
                        transactions,
                    },
                ) in model_output
                {
                    for (_, tx) in transactions {
                        tax_summary.apply_tx(&tx.tax_tx, tx.amount);
                    }
                }
            }
        }

        let (adjustment, tax_flow) = tax_policy.calculate_adjustment(year, &tax_summary);
        flows
            .entry(tax_category.clone())
            .or_insert_with(Vec::new)
            .push(tax_flow);

        YearlyReport {
            category_summary: summary,
            tax_summary,
            tax_adjustment: adjustment,
        }
    }

    pub fn run(&mut self, time_range: TimeRange<Year>) -> ModelReport {
        let mut category_values: Vec<CategoryValue> = self
            .categories
            .iter()
            .map(|category| category.value())
            .collect();

        let mut out = BTreeMap::new();
        for year in time_range.into_iter() {
            let report = Self::run_year(
                year.clone(),
                &mut category_values,
                &mut self.flows,
                &self.tax_policy,
                &self.tax_category,
            );
            out.insert(year, report);
        }

        ModelReport { years: out }
    }
}

// This models a single category over time
pub struct CategoryModel<'iter, 'model> {
    category_value: &'iter mut CategoryValue<'model>,
    flows: &'iter Vec<Flow>,
}

impl<'a, 'b: 'a> CategoryModel<'a, 'b> {
    pub fn run(&mut self, year: Year) -> BTreeMap<Month, MonthlyReport> {
        let mut all_transactions = BTreeMap::new();
        for time in year.months() {
            let mut months_txns = BTreeMap::new();
            for flow in self.flows.iter() {
                if flow.value.applies_at(&time, flow) {
                    let tx = flow.calculate_transaction(&self.category_value, &time);
                    months_txns.insert(flow.name.clone(), tx);
                }
            }
            for tx in months_txns.values() {
                self.category_value.apply_tx(tx);
            }
            all_transactions.insert(
                time.month.clone(),
                MonthlyReport {
                    value: self.category_value.value(),
                    transactions: months_txns,
                },
            );
        }
        all_transactions
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{anyhow, Context, Result};
    use maplit::btreemap;
    use std::collections::BTreeSet;

    use itertools::enumerate;

    use crate::asset::{Asset, AssetName, Rate};
    use crate::flow::FixedFlow;
    use crate::tax::{ConstantTaxPolicy, FixedRateTaxPolicy};
    use crate::time::{Frequency, Month, Time, TimeNext};

    fn test_flow(n: i64, month: Month, frequency: Frequency, value: Money) -> Flow {
        let start = Time {
            year: Year(2021),
            month,
        };
        Flow {
            name: FlowName(n.to_string()),
            description: "A unit test flow".to_string(),
            start: start.clone(),
            end: Time {
                year: Year(2023),
                month: start.month,
            },
            frequency,
            value: Box::new(FixedFlow { value }),
            tax_policy: Box::new(ConstantTaxPolicy {
                rate: Rate::from_percent(10),
            }),
        }
    }

    fn verify_year(
        mut out: BTreeMap<Month, MonthlyReport>,
        initial_value: Money,
        tax_delta: Option<Money>,
        values: Vec<(Vec<i64>, Vec<&'static str>)>,
    ) -> Result<()> {
        let mut start = Month::January;
        let mut value = initial_value;
        for (n, (new_values, flow_names)) in enumerate(values.into_iter()) {
            let report = out.remove(&start).unwrap();

            let flow_values: Vec<i64> = report
                .transactions
                .iter()
                .map(|f| f.1.amount.as_cents())
                .collect();

            let out_flow_names: BTreeSet<FlowName> =
                report.transactions.into_iter().map(|f| f.0).collect();

            let mut flow_names: BTreeSet<FlowName> = flow_names
                .into_iter()
                .map(|f| FlowName(f.to_string()))
                .collect();

            let new_values: Vec<i64> = new_values
                .into_iter()
                .map(|v| {
                    Money::from_dollars(v)
                        .at_rate(Rate::from_percent(90))
                        .as_cents()
                })
                .collect();
            let report_delta: Money = flow_values.iter().map(|c| Money::from_cents(*c)).sum();
            let mut delta = Money::from_cents(new_values.iter().sum());
            if start == Month::April {
                if let Some(tax_delta) = &tax_delta {
                    flow_names.insert(FlowName("Tax adjustment".to_string()));
                    delta = delta + *tax_delta;
                }
            }

            if flow_names != out_flow_names {
                return Err(anyhow!(
                    "Flows were different for {} {:?}:\n{:?}\n{:?}",
                    n,
                    start,
                    out_flow_names,
                    flow_names,
                ));
            }

            value = value + delta;
            if report.value != value {
                return Err(anyhow!(
                    "Total value was different for {} {:?}:\n{} = {} + {:?}\n{} = {} + {:?}",
                    n,
                    start,
                    // report
                    report.value,
                    report.value - report_delta,
                    flow_values,
                    // test values
                    value,
                    value - delta,
                    new_values,
                ));
            }
            start = start.next();
        }
        if !out.is_empty() {
            Err(anyhow!("Element remained in report after test: {:?}", out))
        } else {
            Ok(())
        }
    }

    #[test]
    fn test_model() -> Result<()> {
        let c1 = Category::from_assets(
            CategoryName("c1".to_string()),
            vec![Asset {
                name: AssetName("a1".to_string()),
                value: Money::from_dollars(123),
            }],
        );
        let c2 = Category::from_assets(
            CategoryName("c2".to_string()),
            vec![Asset {
                name: AssetName("a1".to_string()),
                value: Money::from_dollars(456),
            }],
        );

        let tax_policy = FixedRateTaxPolicy::new(Rate::from_percent(35), Money::from_dollars(3000));

        let flows = btreemap! {
            c1.name.clone() => vec![
                test_flow(0, Month::January, Frequency::Monthly, Money::from_dollars(1)),
                test_flow(1, Month::January, Frequency::Monthly, Money::from_dollars(20)),
                test_flow(2, Month::March, Frequency::Quarterly, Money::from_dollars(300)),
                test_flow(3, Month::July, Frequency::Yearly, Money::from_dollars(4000)),
            ],
            c2.name.clone() => vec![
                test_flow(4, Month::February, Frequency::Monthly, Money::from_dollars(5)),
                test_flow(5, Month::March, Frequency::Monthly, Money::from_dollars(60)),
                test_flow(6, Month::May, Frequency::Quarterly, Money::from_dollars(700)),
                test_flow(7, Month::July, Frequency::Yearly, Money::from_dollars(8000)),
            ]
        };

        let tax_category = c1.name.clone();
        let mut model = Model::new(
            flows,
            vec![c1.clone(), c2.clone()],
            Box::new(tax_policy),
            tax_category,
        )
        .context("failed to build model")?;

        let mut out = model.run(TimeRange {
            start: Year(2020),
            end: Year(2024),
        });
        println!("{:#?}", out);

        let mut empty_year = vec![];
        for _ in 0..12 {
            empty_year.push((vec![], vec![]));
        }

        fn c1_year() -> Vec<(Vec<i64>, Vec<&'static str>)> {
            vec![
                (vec![1, 20], vec!["0", "1"]),            // Jan
                (vec![1, 20], vec!["0", "1"]),            // Feb
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Mar
                (vec![1, 20], vec!["0", "1"]),            // Apr
                (vec![1, 20], vec!["0", "1"]),            // May
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Jun
                (vec![1, 20, 4000], vec!["0", "1", "3"]), // Jul
                (vec![1, 20], vec!["0", "1"]),            // Aug
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Sep
                (vec![1, 20], vec!["0", "1"]),            // Oct
                (vec![1, 20], vec!["0", "1"]),            // Nov
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Dec
            ]
        }

        fn c1_yearly(tax_value: i64) -> Money {
            Money::from_dollars(1 * 12 + 20 * 12 + 300 * 4 + 4000 + tax_value)
        }

        fn c2_yearly(first_year: bool) -> Money {
            if first_year {
                Money::from_dollars(5 * 11 + 60 * 10 + 700 * 3 + 8000)
            } else {
                Money::from_dollars(5 * 12 + 60 * 12 + 700 * 4 + 8000)
            }
        }

        let withheld_rate = Rate::from_percent(10);
        let net_rate = withheld_rate.inverse();

        let tax_2021 = Money::from_cents(-300175);
        let tax_2022 = Money::from_cents(-320800);
        let values = btreemap! {
            Year(2020) => (
                None,
                TaxSummary {
                    net_amount: Money::from_dollars(0),
                    taxable_income: Money::from_dollars(0),
                    tax_withheld: Money::from_dollars(0),
                },
                TaxAdjustment {
                    owed: Money::from_dollars(0),
                    withheld: Money::from_dollars(0),
                    delta: Money::from_dollars(0),
                    effective_rate: Rate::from_percent(0),
                },
                btreemap!{
                    c1.name.clone() => (Money::from_dollars(123), empty_year.clone()),
                    c2.name.clone() => (Money::from_dollars(456), empty_year.clone()),
                },
            ),
            Year(2021) => (
                Some(Money::from_dollars(0)),
                TaxSummary {
                    net_amount: (c1_yearly(0) + c2_yearly(true)).at_rate(net_rate),
                    taxable_income: c1_yearly(0) + c2_yearly(true),
                    tax_withheld: (c1_yearly(0) + c2_yearly(true)).at_rate(withheld_rate),
                },
                // Tax from 2021 should be c1_yearly ($5,452) + c2_yearly ($10,755) = $16,207 gross income.
                // We have $3,000 in deductions so taxable income is $13,207. Taxed at 35% we owe $4,622.45
                // in tax. We withhold at 10% so we have withheld $1620.7 and the delta for refund/debt is
                // $3,001.75
                TaxAdjustment {
                    owed: Money::from_cents(462245),
                    withheld: Money::from_cents(162070),
                    delta: tax_2021,
                    effective_rate: Rate::from_percent(35),
                },
                // Last year we made 0 dollars to we should be able to start with the same values.
                btreemap!{
                    c1.name.clone() => (Money::from_dollars(123), c1_year()),
                    c2.name.clone() => (
                        Money::from_dollars(456),
                        vec![
                            (vec![], vec![]),                         // Jan
                            (vec![5], vec!["4"]),                     // Feb
                            (vec![5, 60], vec!["4", "5"]),            // Mar
                            (vec![5, 60], vec!["4", "5"]),            // Apr
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // May
                            (vec![5, 60], vec!["4", "5"]),            // Jun
                            (vec![5, 60, 8000], vec!["4", "5", "7"]), // Jul
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // Aug
                            (vec![5, 60], vec!["4", "5"]),            // Sep
                            (vec![5, 60], vec!["4", "5"]),            // Oct
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // Nov
                            (vec![5, 60], vec!["4", "5"]),            // Dec
                        ]
                    ),
                },
            ),
            Year(2022) => (
                Some(tax_2021),
                TaxSummary {
                    net_amount: (c1_yearly(0) + c2_yearly(false)).at_rate(net_rate) + tax_2021,
                    taxable_income: c1_yearly(0) + c2_yearly(false),
                    tax_withheld: (c1_yearly(0) + c2_yearly(false)).at_rate(withheld_rate),
                },
                // Tax from 2022 should be c1_yearly ($5,452) + c2_yearly ($11,580) = $17,032 gross income.
                // We have $3,000 in deductions so taxable income is $14,032. Taxed at 35% we owe $4,911.20
                // in tax. We withhold at 10% so we have withheld $1703.2 and the delta for refund/debt is
                // $3,208
                TaxAdjustment {
                    owed: Money::from_cents(491120),
                    withheld: Money::from_cents(170320),
                    delta: tax_2022,
                    effective_rate: Rate::from_percent(35),
                },
                btreemap!{
                    c1.name.clone() => (Money::from_dollars(123) + c1_yearly(0).at_rate(net_rate), c1_year()),
                    c2.name.clone() => (
                        Money::from_dollars(456) + c2_yearly(true).at_rate(net_rate),
                        vec![
                            (vec![5, 60], vec!["4", "5"]),            // Jan
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // Feb
                            (vec![5, 60], vec!["4", "5"]),            // Mar
                            (vec![5, 60], vec!["4", "5"]),            // Apr
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // May
                            (vec![5, 60], vec!["4", "5"]),            // Jun
                            (vec![5, 60, 8000], vec!["4", "5", "7"]), // Jul
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // Aug
                            (vec![5, 60], vec!["4", "5"]),            // Sep
                            (vec![5, 60], vec!["4", "5"]),            // Oct
                            (vec![5, 60, 700], vec!["4", "5", "6"]),  // Nov
                            (vec![5, 60], vec!["4", "5"]),            // Dec
                        ]
                    ),
                },
            ),
            Year(2023) => (
                Some(tax_2022),
                TaxSummary {
                    net_amount: Money::from_dollars(5 + 60 + 60 + 700).at_rate(net_rate) + tax_2022,
                    taxable_income: Money::from_dollars(5 + 60 + 60 + 700),
                    tax_withheld: Money::from_dollars(5 + 60 + 60 + 700).at_rate(withheld_rate),
                },
                // Tax from 2023 should be c1_yearly ($0) + c2_yearly ($825) = $825 gross income.
                // We have $3,000 in deductions so taxable income is $0. Taxed at 35% we owe $0 in tax.
                // We withhold at 10% so we have withheld $82.50 and the delta for refund/debt is
                // $82.50
                TaxAdjustment {
                    owed: Money::from_cents(0),
                    withheld: Money::from_cents(8250),
                    delta: Money::from_cents(8250),
                    effective_rate: Rate::from_percent(0),
                },
                btreemap!{
                    c1.name.clone() => (Money::from_dollars(123) + c1_yearly(0).at_rate(net_rate) + c1_yearly(0).at_rate(net_rate) + tax_2021, empty_year.clone()),
                    c2.name.clone() => (
                        Money::from_dollars(456) + c2_yearly(true).at_rate(net_rate) + c2_yearly(false).at_rate(net_rate),
                        vec![
                            (vec![5, 60], vec!["4", "5"]),   // Jan
                            (vec![60, 700], vec!["5", "6"]), // Feb
                            (vec![], vec![]),                // Mar
                            (vec![], vec![]),                // Apr
                            (vec![], vec![]),                // May
                            (vec![], vec![]),                // Jun
                            (vec![], vec![]),                // Jul
                            (vec![], vec![]),                // Aug
                            (vec![], vec![]),                // Sep
                            (vec![], vec![]),                // Oct
                            (vec![], vec![]),                // Nov
                            (vec![], vec![]),                // Dec
                        ]
                    ),
                },
            ),
        };

        for (year, (prev_tax, tax_summary, tax_adjustment, mut data)) in values {
            let mut report = out.years.remove(&year).unwrap();

            let c1_report = report.category_summary.remove(&c1.name).unwrap();
            let c2_report = report.category_summary.remove(&c2.name).unwrap();

            let (c1_start_value, c1_deltas) = data.remove(&c1.name).unwrap();
            verify_year(c1_report, c1_start_value, prev_tax, c1_deltas)
                .context(format!("Failed to verify c1 {:?}", year))?;

            let (c2_start_value, c2_deltas) = data.remove(&c2.name).unwrap();
            verify_year(c2_report, c2_start_value, None, c2_deltas)
                .context(format!("Failed to verify c2 {:?}", year))?;

            assert!(report.category_summary.is_empty());

            assert_eq!(
                (year, report.tax_summary.net_amount),
                (year, tax_summary.net_amount)
            );
            assert_eq!(
                (year, report.tax_summary.taxable_income),
                (year, tax_summary.taxable_income)
            );
            assert_eq!(
                (year, report.tax_summary.tax_withheld),
                (year, tax_summary.tax_withheld)
            );

            assert_eq!(
                (year, report.tax_adjustment.owed),
                (year, tax_adjustment.owed)
            );
            assert_eq!(
                (year, report.tax_adjustment.withheld),
                (year, tax_adjustment.withheld)
            );
            assert_eq!(
                (year, report.tax_adjustment.delta),
                (year, tax_adjustment.delta)
            );
            assert_eq!(
                (year, report.tax_adjustment.effective_rate),
                (year, tax_adjustment.effective_rate)
            );
        }
        assert!(out.years.is_empty());

        Ok(())
    }

    #[test]
    fn test_category_model() -> Result<()> {
        let cat = Category::from_assets(
            CategoryName("unittest".to_string()),
            vec![Asset {
                name: AssetName("unit test asset".to_string()),
                value: Money::from_dollars(123),
            }],
        );

        let mut distant_flow = test_flow(
            4,
            Month::January,
            Frequency::Monthly,
            Money::from_dollars(600000),
        );
        distant_flow.start.year = Year(2022);
        distant_flow.end.year = Year(2022);

        let flows = vec![
            test_flow(
                0,
                Month::January,
                Frequency::Monthly,
                Money::from_dollars(1),
            ),
            test_flow(
                1,
                Month::January,
                Frequency::Monthly,
                Money::from_dollars(20),
            ),
            test_flow(
                2,
                Month::March,
                Frequency::Quarterly,
                Money::from_dollars(300),
            ),
            test_flow(3, Month::July, Frequency::Yearly, Money::from_dollars(4000)),
            distant_flow,
        ];

        let mut cat_model = CategoryModel {
            category_value: &mut cat.value(),
            flows: &flows,
        };

        verify_year(
            cat_model.run(Year(2021)),
            Money::from_dollars(123),
            None,
            vec![
                (vec![1, 20], vec!["0", "1"]),            // Jan
                (vec![1, 20], vec!["0", "1"]),            // Feb
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Mar
                (vec![1, 20], vec!["0", "1"]),            // Apr
                (vec![1, 20], vec!["0", "1"]),            // May
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Jun
                (vec![1, 20, 4000], vec!["0", "1", "3"]), // Jul
                (vec![1, 20], vec!["0", "1"]),            // Aug
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Sep
                (vec![1, 20], vec!["0", "1"]),            // Oct
                (vec![1, 20], vec!["0", "1"]),            // Nov
                (vec![1, 20, 300], vec!["0", "1", "2"]),  // Dec
            ],
        )
    }
}
