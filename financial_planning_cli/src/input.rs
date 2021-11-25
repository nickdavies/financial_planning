use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use financial_planning_lib::asset::{Asset, AssetName, Category, CategoryName, Money, Rate};
use financial_planning_lib::flow::{FixedFlow, Flow, FlowName, FlowValue, RateFlow};
use financial_planning_lib::model::Model;
use financial_planning_lib::tax::{
    AnnualTaxPolicy, ConstantTaxPolicy, FixedRateTaxPolicy, NoWithholding, TaxExempt, TaxPolicy,
};
use financial_planning_lib::time::{Time, TimeRange, Year};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub time_range: YearRange,
    pub tax: AnnualTaxPolicyRaw,
    pub common: PlanCommon,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YearRange {
    start: u32,
    end: u32,
}

impl TryFrom<YearRange> for TimeRange<Year> {
    type Error = anyhow::Error;

    fn try_from(other: YearRange) -> Result<Self, Self::Error> {
        Ok(TimeRange {
            start: Year(other.start),
            end: Year(other.end),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "policy")]
pub enum AnnualTaxPolicyRaw {
    #[serde(rename = "fixed_rate")]
    FixedRate { rate: i64, standard_deduction: i64 },
}

impl TryFrom<AnnualTaxPolicyRaw> for Box<dyn AnnualTaxPolicy> {
    type Error = anyhow::Error;

    fn try_from(other: AnnualTaxPolicyRaw) -> Result<Self, Self::Error> {
        Ok(Box::new(match other {
            AnnualTaxPolicyRaw::FixedRate {
                rate,
                standard_deduction,
            } => FixedRateTaxPolicy::new(
                Rate::from_percent(rate),
                Money::from_dollars(standard_deduction),
            ),
        }))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCommon {
    pub categories: Vec<String>,
    pub tax_category: String,
    pub assets_file: PathBuf,
    pub flows_file: PathBuf,
    pub times_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRaw {
    category: String,
    value: i64,
}

impl AssetRaw {
    fn build(self, name: String) -> Result<Asset> {
        Ok(Asset {
            name: AssetName(name),
            value: Money::from_dollars(self.value),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(transparent)]
pub struct Assets {
    assets: BTreeMap<String, AssetRaw>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(transparent)]
pub struct TimesTable {
    times: BTreeMap<String, TimeLiteral>,
}

impl TimesTable {
    fn get_by_name(&self, name: &str) -> Result<Time> {
        let lit = self.times.get(name).context(format!(
            "Unknown named time \"{}\" options are {:?}",
            name,
            self.times.keys()
        ))?;

        lit.clone()
            .try_into()
            .context(format!("Failed to parse time for time \"{}\"", name))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum TimeRaw {
    Literal(TimeLiteral),
    Named(String),
}

impl TimeRaw {
    fn build(self, times_table: &TimesTable) -> Result<Time> {
        Ok(match self {
            Self::Literal(lit) => (&lit)
                .try_into()
                .context("failed to build time from literal")?,
            Self::Named(name) => times_table
                .get_by_name(&name)
                .context("Failed to parse named time")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeLiteral {
    year: u32,
    month: String,
}

impl TryFrom<&TimeLiteral> for Time {
    type Error = anyhow::Error;

    fn try_from(other: &TimeLiteral) -> Result<Self, Self::Error> {
        Ok(Time {
            year: Year(other.year),
            month: other.month.parse().context("Failed to parse month")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum FlowValueRaw {
    #[serde(rename = "fixed")]
    FixedFlow { value: i64 },
    #[serde(rename = "rate")]
    RateFlow { rate: i64 },
}

impl TryFrom<FlowValueRaw> for Box<dyn FlowValue> {
    type Error = anyhow::Error;

    fn try_from(other: FlowValueRaw) -> Result<Self, Self::Error> {
        Ok(match other {
            FlowValueRaw::FixedFlow { value } => Box::new(FixedFlow {
                value: Money::from_dollars(value),
            }),
            FlowValueRaw::RateFlow { rate } => Box::new(RateFlow {
                rate: Rate::from_percent(rate),
            }),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "policy")]
pub enum FlowTaxPolicy {
    #[serde(rename = "no_withholding")]
    NoWithholding,
    #[serde(rename = "tax_exempt")]
    TaxExempt,
    #[serde(rename = "fixed_rate")]
    FixedRate { rate: i64 },
}

impl TryFrom<FlowTaxPolicy> for Box<dyn TaxPolicy> {
    type Error = anyhow::Error;

    fn try_from(other: FlowTaxPolicy) -> Result<Self, Self::Error> {
        Ok(match other {
            FlowTaxPolicy::NoWithholding => Box::new(NoWithholding {}),
            FlowTaxPolicy::TaxExempt => Box::new(TaxExempt {}),
            FlowTaxPolicy::FixedRate { rate } => Box::new(ConstantTaxPolicy {
                rate: Rate::from_percent(rate),
            }),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRaw {
    description: String,
    category: String,
    start: TimeRaw,
    end: TimeRaw,
    frequency: String,
    value: FlowValueRaw,
    tax: FlowTaxPolicy,
}

impl FlowRaw {
    fn build(self, name: String, times_table: &TimesTable) -> Result<Flow> {
        Ok(Flow {
            name: FlowName(name),
            description: self.description,
            start: self
                .start
                .build(times_table)
                .context("Failed to convert start time")?,
            end: self
                .end
                .build(times_table)
                .context("Failed to convert end time")?,
            frequency: self.frequency.parse().context("")?,
            value: self.value.try_into().context("")?,
            tax_policy: self.tax.try_into().context("")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(transparent)]
pub struct Flows {
    flows: BTreeMap<String, FlowRaw>,
}

impl Flows {
    fn build(self, times_table: &TimesTable) -> Result<BTreeMap<CategoryName, Vec<Flow>>> {
        let mut out = BTreeMap::new();

        for (flow_name, flow_raw) in self.flows.into_iter() {
            out.entry(CategoryName(flow_raw.category.clone()))
                .or_insert_with(Vec::new)
                .push(
                    flow_raw
                        .build(flow_name.clone(), times_table)
                        .context(format!("Failed to build flow \"{}\"", flow_name))?,
                )
        }

        Ok(out)
    }
}

#[derive(Debug)]
pub struct Config {
    plan: Plan,
    assets: Assets,
    flows: Flows,
    times_table: TimesTable,
}

impl Config {
    fn build_categories(listed_categories: &[String], assets: Assets) -> Result<Vec<Category>> {
        let mut cat_map = BTreeMap::new();
        for category in listed_categories {
            cat_map.insert(category.clone(), Vec::new());
        }

        for (asset_name, asset) in assets.assets.into_iter() {
            println!("HERE: {:?}\n{:?}", asset, cat_map);
            match cat_map.get_mut(&asset.category) {
                Some(new_assets) => {
                    new_assets.push(asset.build(asset_name).context("Failed to build asset")?)
                }
                None => {
                    return Err(anyhow!(
                        "Asset found with category \"{}\" which isn't listed in categories ({:?})",
                        asset.category,
                        listed_categories,
                    ));
                }
            }
        }

        Ok(cat_map
            .into_iter()
            .map(|(name, assets)| Category::from_assets(CategoryName(name), assets))
            .collect())
    }

    pub fn build_model(self) -> Result<(TimeRange<Year>, Model)> {
        let categories = Self::build_categories(&self.plan.common.categories, self.assets)
            .context("Failed to build categories")?;

        Ok((
            self.plan
                .time_range
                .try_into()
                .context("Failed to convert time range")?,
            Model::new(
                self.flows
                    .build(&self.times_table)
                    .context("Failed to convert flows")?,
                categories,
                self.plan
                    .tax
                    .try_into()
                    .context("Failed to build tax policy")?,
                CategoryName(self.plan.common.tax_category),
            )
            .context("Failed to build model")?,
        ))
    }
}

fn load_subfile<T>(name: &str, plan_file: &Path, relative: &Path) -> Result<T>
where
    for<'a> T: serde::Deserialize<'a>,
{
    let subfile_path = plan_file
        .parent()
        .context("Failed to remove filename from provided plan config path")?
        .join(&relative);

    Ok(toml::from_str(
        &std::fs::read_to_string(&subfile_path)
            .context(format!("Failed to read {} file contents", name))?,
    )
    .context(format!("Failed to parse {} config", name))?)
}

pub fn read_configs(plan_file: &Path) -> Result<Config> {
    let plan: Plan = toml::from_str(
        &std::fs::read_to_string(plan_file).context("Failed to read plan file contents")?,
    )
    .context("Failed to parse plan config")?;

    Ok(Config {
        assets: load_subfile("assets", plan_file, &plan.common.assets_file)?,
        flows: load_subfile("flows", plan_file, &plan.common.flows_file)?,
        times_table: match &plan.common.times_file {
            Some(file) => load_subfile("times", plan_file, &file)?,
            None => TimesTable::default(),
        },
        plan,
    })
}
