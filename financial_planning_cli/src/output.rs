use std::collections::BTreeSet;

use anyhow::{Context, Result};
use structopt::StructOpt;

use financial_planning_lib::asset::Money;
use financial_planning_lib::model::{CategoriesSnapshot, ModelReport, YearlyReport};
use financial_planning_lib::time::{TimeRange, Year};

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
    /// Print out a summary for each simulated month
    Monthly {
        #[structopt(long)]
        include_tax: bool,

        #[structopt(long)]
        include_flows: bool,
    },
}

impl OutputType {
    pub fn output(&self, report: ModelReport, time_range: &TimeRange<Year>) -> Result<()> {
        match self {
            Self::Debug => {
                println!("{:#?}", report);
            }
            Self::EndOnly => {
                println!(
                    "Ran model for: {} -> {}",
                    time_range.start.0, time_range.end.0
                );
                Self::print_category_changes(&report.start_values, &report.end_values)
                    .context("failed to merge categories, this is a bug!")?;
            }
            Self::Yearly { include_tax } => {
                for (year, yearly_report) in report.years {
                    Self::print_yearly_summaries(year, &yearly_report, *include_tax)?;
                }
            }
            Self::Monthly {
                include_tax,
                include_flows,
            } => {
                for (year, yearly_report) in report.years {
                    Self::print_yearly_summaries(year, &yearly_report, *include_tax)?;
                    println!("## Monthly breakdown for {}", year.0);
                    for month in year.months() {
                        for (category, monthly_reports) in yearly_report.category_summary.iter() {
                            if let Some(monthly_report) = monthly_reports.get(&month.month) {
                                println!(
                                    "  {:?} {} = {} => {} ({})",
                                    month.month,
                                    category.0,
                                    monthly_report.start_value,
                                    monthly_report.end_value,
                                    monthly_report.end_value - monthly_report.start_value,
                                );
                                if *include_flows {
                                    for (flow, tx) in &monthly_report.transactions {
                                        println!(
                                            "    {}: {}{}",
                                            flow.0,
                                            tx.amount,
                                            if *include_tax {
                                                format!(
                                                    " ({} tax withheld and {} taxable income)",
                                                    tx.tax_tx.tax_withheld,
                                                    tx.tax_tx.taxable_income
                                                )
                                            } else {
                                                "".to_string()
                                            }
                                        );
                                    }
                                    println!("");
                                }
                            }
                        }
                    }
                    println!("");
                }
            }
        }
        Ok(())
    }

    fn print_category_changes(start: &CategoriesSnapshot, end: &CategoriesSnapshot) -> Result<()> {
        let mut keys: BTreeSet<_> = start.keys().collect();
        keys.extend(end.keys());

        let mut total_start = Money::from_dollars(0);
        let mut total_end = Money::from_dollars(0);
        for key in keys {
            let start_value = start
                .get(&key)
                .context(format!("Provided start snapshot doesn't contain {:?}", key))?;

            let end_value = end
                .get(&key)
                .context(format!("Provided end snapshot doesn't contain {:?}", key))?;

            total_start = total_start + *start_value;
            total_end = total_end + *end_value;

            println!(
                "  {} = {} => {} ({})",
                key.0,
                start_value,
                end_value,
                *end_value - *start_value
            );
        }
        println!("");
        println!(
            "  TOTAL NW: {} => {} ({})",
            total_start,
            total_end,
            total_end - total_start
        );
        Ok(())
    }

    fn print_yearly_summaries(
        year: Year,
        yearly_report: &YearlyReport,
        include_tax: bool,
    ) -> Result<()> {
        println!("# {} yearly category summary", year.0);
        Self::print_category_changes(&yearly_report.start_values, &yearly_report.end_values)
            .context("failed to merge categories, this is a bug!")?;
        println!("");

        if include_tax {
            println!("# {} yearly tax summary:", year.0);
            println!(
                "  Change in wealth: {}",
                yearly_report.tax_summary.net_amount
            );
            println!(
                "  taxable income: {}",
                yearly_report.tax_summary.taxable_income
            );
            println!("  tax withheld: {}", yearly_report.tax_summary.tax_withheld);
            println!("  tax owed: {}", yearly_report.tax_adjustment.owed);
            println!("  tax delta: {}", yearly_report.tax_adjustment.delta);
            println!(
                "  tax rate: {}",
                yearly_report.tax_adjustment.effective_rate
            );
            println!("");
        }
        Ok(())
    }
}
