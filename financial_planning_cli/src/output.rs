use std::collections::BTreeSet;

use anyhow::{Context, Result};
use structopt::StructOpt;

use financial_planning_lib::model::{CategoriesSnapshot, ModelReport};

#[derive(Debug, StructOpt)]
pub enum OutputType {
    /// Debug print every detail you have
    Debug,
    /// Only print out the final state
    EndOnly,
    /// Print out a summary for every simulated year
    Yearly {
        #[structopt(long)]
        include_tax: bool,
    },
}

impl OutputType {
    pub fn output(&self, report: ModelReport) -> Result<()> {
        match self {
            Self::Debug => {
                println!("{:#?}", report);
            }
            Self::EndOnly => {
                Self::print_category_changes(&report.start_values, &report.end_values)
                    .context("failed to merge categories, this is a bug!")?;
            }
            Self::Yearly { include_tax } => {
                for (year, yearly_report) in report.years {
                    println!("# {} category summary", year.0);
                    Self::print_category_changes(
                        &yearly_report.start_values,
                        &yearly_report.end_values,
                    )
                    .context("failed to merge categories, this is a bug!")?;
                    println!("");

                    if *include_tax {
                        println!("# {} tax summary:", year.0);
                        println!("Change in wealth: {}", yearly_report.tax_summary.net_amount);
                        println!(
                            "taxable income: {}",
                            yearly_report.tax_summary.taxable_income
                        );
                        println!("tax withheld: {}", yearly_report.tax_summary.tax_withheld);
                        println!("tax owed: {}", yearly_report.tax_adjustment.owed);
                        println!("tax delta: {}", yearly_report.tax_adjustment.delta);
                        println!("tax rate: {}", yearly_report.tax_adjustment.effective_rate);
                        println!("");
                    }
                }
            }
        }
        Ok(())
    }

    fn print_category_changes(start: &CategoriesSnapshot, end: &CategoriesSnapshot) -> Result<()> {
        let mut keys: BTreeSet<_> = start.keys().collect();
        keys.extend(end.keys());

        for key in keys {
            let start_value = start
                .get(&key)
                .context(format!("Provided start snapshot doesn't contain {:?}", key))?;

            let end_value = end
                .get(&key)
                .context(format!("Provided end snapshot doesn't contain {:?}", key))?;

            println!("{} = {} => {}", key.0, start_value, end_value);
        }
        Ok(())
    }
}
