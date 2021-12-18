use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use financial_planning_lib::asset::{
    Asset, AssetName, Category, CategoryBound, CategoryName, Money, Rate,
};
use financial_planning_lib::events::{BuildFlows, EventName, HousePurchase};
use financial_planning_lib::flow::{
    FixedFlow, Flow, FlowName, FlowValue, RateFlow, RateTableFlow, TableFlow, UnitsTableFlow,
};
use financial_planning_lib::lookup_table::LookupTable;
use financial_planning_lib::model::Model;
use financial_planning_lib::tax::{
    AnnualTaxPolicy, ConstantTaxPolicy, FixedRateTaxPolicy, NoWithholding, PartiallyTaxed,
    TaxExempt, TaxPolicy,
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
    FixedRate {
        rate: String,
        standard_deduction: i64,
    },
}

impl TryFrom<AnnualTaxPolicyRaw> for Box<dyn AnnualTaxPolicy> {
    type Error = anyhow::Error;

    fn try_from(other: AnnualTaxPolicyRaw) -> Result<Self, Self::Error> {
        Ok(Box::new(match other {
            AnnualTaxPolicyRaw::FixedRate {
                rate,
                standard_deduction,
            } => FixedRateTaxPolicy::new(
                rate.parse().context("Failed to parse rate")?,
                Money::from_dollars(standard_deduction),
            ),
        }))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCommon {
    pub categories: Vec<CategoryTableRaw>,
    pub tax_category: String,
    pub assets_file: PathBuf,
    pub flows_file: PathBuf,
    pub events_file: Option<PathBuf>,
    pub times_file: Option<PathBuf>,
    pub tables_file: Option<PathBuf>,
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
    RateFlow { rate: String },
    #[serde(rename = "table")]
    TableFlow { table_name: String },
    #[serde(rename = "rate_table")]
    RateTableFlow { table_name: String },
    #[serde(rename = "units_table")]
    UnitsTableFlow { table_name: String, units: i64 },
}

impl FlowValueRaw {
    fn build(self, tables: &BTreeMap<String, TableType>) -> Result<Box<dyn FlowValue>> {
        Ok(match self {
            Self::FixedFlow { value } => Box::new(FixedFlow {
                value: Money::from_dollars(value),
            }),
            Self::RateFlow { rate } => Box::new(RateFlow {
                rate: rate.parse().context("Failed to parse provided rate")?,
            }),
            Self::TableFlow { table_name } => Box::new(TableFlow {
                table: match tables.get(&table_name) {
                    Some(TableType::Money(t)) => t.clone(),
                    Some(TableType::Rate(_)) => {
                        return Err(anyhow!("Found table {} but it's a rate table not money table, did you mean to use rate_table?", table_name));
                    }
                    None => {
                        return Err(anyhow!("Unknown table {}", table_name));
                    }
                },
            }),
            Self::RateTableFlow { table_name } => Box::new(RateTableFlow {
                table: match tables.get(&table_name) {
                    Some(TableType::Rate(t)) => t.clone(),
                    Some(TableType::Money(_)) => {
                        return Err(anyhow!("Found table {} but it's a money table not rate table, did you mean to use table?", table_name));
                    }
                    None => {
                        return Err(anyhow!("Unknown table {}", table_name));
                    }
                },
            }),
            Self::UnitsTableFlow { table_name, units } => Box::new(UnitsTableFlow {
                units: units,
                table: match tables.get(&table_name) {
                    Some(TableType::Money(t)) => t.clone(),
                    Some(TableType::Rate(_)) => {
                        return Err(anyhow!(
                            "Found table {} but it's a rate table not money table",
                            table_name
                        ));
                    }
                    None => {
                        return Err(anyhow!("Unknown table {}", table_name));
                    }
                },
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
    FixedRate { rate: String },
    #[serde(rename = "partially_taxed")]
    PartiallyTaxed {
        taxed_proportion: String,
        withholding_rate: String,
    },
}

impl TryFrom<FlowTaxPolicy> for Box<dyn TaxPolicy> {
    type Error = anyhow::Error;

    fn try_from(other: FlowTaxPolicy) -> Result<Self, Self::Error> {
        Ok(match other {
            FlowTaxPolicy::NoWithholding => Box::new(NoWithholding {}),
            FlowTaxPolicy::TaxExempt => Box::new(TaxExempt {}),
            FlowTaxPolicy::FixedRate { rate } => Box::new(ConstantTaxPolicy {
                rate: rate.parse().context("failed to parse provided rate")?,
            }),
            FlowTaxPolicy::PartiallyTaxed {
                taxed_proportion,
                withholding_rate,
            } => Box::new(PartiallyTaxed {
                taxed_proportion: taxed_proportion
                    .parse()
                    .context("failed to parse provided taxed_proportion")?,
                withholding_rate: withholding_rate
                    .parse()
                    .context("failed to parse provided withholding_rate")?,
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
    fn build(
        self,
        name: String,
        times_table: &TimesTable,
        lookup_tables: &BTreeMap<String, TableType>,
    ) -> Result<Flow> {
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
            frequency: self
                .frequency
                .parse()
                .context("Failed to convert frequency")?,
            value: self
                .value
                .build(lookup_tables)
                .context("Failed to convert value")?,
            tax_policy: self
                .tax
                .try_into()
                .context("Failed to convert tax policy")?,
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
    fn build(
        self,
        times_table: &TimesTable,
        lookup_tables: &BTreeMap<String, TableType>,
    ) -> Result<BTreeMap<CategoryName, Vec<Flow>>> {
        let mut out = BTreeMap::new();

        for (flow_name, flow_raw) in self.flows.into_iter() {
            out.entry(CategoryName(flow_raw.category.clone()))
                .or_insert_with(Vec::new)
                .push(
                    flow_raw
                        .build(flow_name.clone(), times_table, lookup_tables)
                        .context(format!("Failed to build flow \"{}\"", flow_name))?,
                )
        }

        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum EventRaw {
    #[serde(rename = "house_purchase")]
    HousePurchase {
        property_name: String,
        start: TimeRaw,
        end: TimeRaw,
        mortgage_rate: String,
        purchase_price: i64,
        setup_cost: i64,
        down_payment: i64,
        property_tax_rate: Option<String>,
        house_value_category: String,
        mortgage_category: String,
        down_payment_category: String,
        regular_payment_category: String,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(transparent)]
pub struct Events {
    events: BTreeMap<String, EventRaw>,
}

impl Events {
    fn build(
        self,
        times_table: &TimesTable,
        _: &BTreeMap<String, TableType>,
    ) -> Result<BTreeMap<EventName, Box<dyn BuildFlows>>> {
        let mut out: BTreeMap<EventName, Box<dyn BuildFlows>> = BTreeMap::new();

        for (event_name, event) in self.events.into_iter() {
            out.insert(
                EventName(event_name),
                match event {
                    EventRaw::HousePurchase {
                        property_name,
                        start,
                        end,
                        mortgage_rate,
                        property_tax_rate,
                        purchase_price,
                        setup_cost,
                        down_payment,
                        down_payment_category,
                        house_value_category,
                        mortgage_category,
                        regular_payment_category,
                    } => Box::new(HousePurchase {
                        property_name,
                        time_range: TimeRange {
                            start: start
                                .build(times_table)
                                .context("failed to build start time")?,
                            end: end.build(times_table).context("failed to build end time")?,
                        },
                        mortgage_rate: mortgage_rate
                            .parse()
                            .context("failed to parse mortgage rate")?,
                        property_tax_rate: match property_tax_rate {
                            Some(r) => {
                                Some(r.parse().context("failed to parse property tax rate")?)
                            }
                            None => None,
                        },
                        purchase_price: Money::from_dollars(purchase_price),
                        setup_cost: Money::from_dollars(setup_cost),
                        down_payment: Money::from_dollars(down_payment),
                        house_value_category: CategoryName(house_value_category),
                        mortgage_category: CategoryName(mortgage_category),
                        down_payment_category: CategoryName(down_payment_category),
                        regular_payment_category: CategoryName(regular_payment_category),
                    }),
                },
            );
        }

        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum TableRaw {
    MonthlyRate {
        monthly_rate: String,
        start: TimeRaw,
        end: TimeRaw,
    },
    YearlyRate {
        yearly_rate: String,
        start: TimeRaw,
        end: TimeRaw,
    },
    Money {
        dollars: i64,
        start: TimeRaw,
        end: TimeRaw,
    },
}

trait Build<T> {
    fn build(self, times_table: &TimesTable) -> Result<T>;
}

impl Build<(TimeRange<Time>, Rate)> for TableRaw {
    fn build(self, times_table: &TimesTable) -> Result<(TimeRange<Time>, Rate)> {
        let (rate, start, end) = match self {
            Self::MonthlyRate {
                monthly_rate,
                start,
                end,
            } => (
                Rate::from_str(&monthly_rate).context("Failed to parse provided rate")?,
                start,
                end,
            ),
            Self::YearlyRate {
                yearly_rate,
                start,
                end,
            } => (
                Rate::from_str(&yearly_rate).context("Failed to parse provided rate")? / 12,
                start,
                end,
            ),
            Self::Money { .. } => {
                return Err(anyhow!("Asked to build a rate table but found money entry"));
            }
        };

        Ok((
            TimeRange {
                start: start
                    .build(times_table)
                    .context("failed to build start time")?,
                end: end.build(times_table).context("failed to build end time")?,
            },
            rate,
        ))
    }
}

impl Build<(TimeRange<Time>, Money)> for TableRaw {
    fn build(self, times_table: &TimesTable) -> Result<(TimeRange<Time>, Money)> {
        match self {
            Self::Money {
                dollars,
                start,
                end,
            } => Ok((
                TimeRange {
                    start: start
                        .build(times_table)
                        .context("failed to build start time")?,
                    end: end.build(times_table).context("failed to build end time")?,
                },
                Money::from_dollars(dollars),
            )),
            Self::MonthlyRate { .. } | Self::YearlyRate { .. } => {
                Err(anyhow!("Asked to build a money table but found rate entry"))
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(transparent)]
pub struct LookupTables {
    tables: BTreeMap<String, Vec<TableRaw>>,
}

#[derive(Debug)]
enum TableType {
    Rate(LookupTable<Time, Rate>),
    Money(LookupTable<Time, Money>),
}

impl LookupTables {
    fn build_table<T>(
        name: &str,
        table_entries: Vec<TableRaw>,
        times_table: &TimesTable,
    ) -> Result<LookupTable<Time, T>>
    where
        TableRaw: Build<(TimeRange<Time>, T)>,
        T: std::fmt::Debug + Clone,
    {
        let mut ranges = Vec::new();
        for (i, entry) in itertools::enumerate(table_entries.into_iter()) {
            ranges.push(
                entry
                    .build(times_table)
                    .context(format!("Failed to build entry {} for table {}", i, name))?,
            )
        }
        LookupTable::new(ranges).context(format!("failed to build table {}", name))
    }

    fn build(self, times_table: &TimesTable) -> Result<BTreeMap<String, TableType>> {
        let mut out = BTreeMap::new();

        for (name, table_entries) in self.tables {
            let first = table_entries
                .iter()
                .next()
                .context(format!("Table {} was somehow empty", name))?;
            let table = match first {
                TableRaw::MonthlyRate { .. } | TableRaw::YearlyRate { .. } => TableType::Rate(
                    Self::build_table(&name, table_entries, times_table).context(
                        "failed to rate table (decided it was rate based on first entry)",
                    )?,
                ),
                TableRaw::Money { .. } => TableType::Money(
                    Self::build_table(&name, table_entries, times_table).context(
                        "failed to money table (decided it was rate based on first entry)",
                    )?,
                ),
            };
            out.insert(name, table);
        }

        Ok(out)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub enum CategoryBoundRaw {
    #[serde(rename = "must_not_go_below_zero")]
    MustNotGoBelowZero,
    #[serde(rename = "must_not_go_above_zero")]
    MustNotGoAboveZero,
}

impl Into<CategoryBound> for CategoryBoundRaw {
    fn into(self: CategoryBoundRaw) -> CategoryBound {
        match self {
            CategoryBoundRaw::MustNotGoBelowZero => CategoryBound::MustNotGoBelowZero,
            CategoryBoundRaw::MustNotGoAboveZero => CategoryBound::MustNotGoAboveZero,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CategoryTableRaw {
    name: String,
    bound: Option<CategoryBoundRaw>,
}

#[derive(Debug)]
pub struct Config {
    plan: Plan,
    assets: Assets,
    flows: Flows,
    events: Events,
    times_table: TimesTable,
    lookup_tables: BTreeMap<String, TableType>,
}

impl Config {
    fn build_categories(
        categories_raw: Vec<CategoryTableRaw>,
        assets: Assets,
    ) -> Result<Vec<Category>> {
        let mut cat_map = BTreeMap::new();
        for category in &categories_raw {
            cat_map.insert(category.name.clone(), Vec::new());
        }

        for (asset_name, asset) in assets.assets.into_iter() {
            match cat_map.get_mut(&asset.category) {
                Some(new_assets) => {
                    new_assets.push(asset.build(asset_name).context("Failed to build asset")?)
                }
                None => {
                    return Err(anyhow!(
                        "Asset found with category \"{}\" which isn't listed in categories ({:?})",
                        asset.category,
                        cat_map.keys(),
                    ));
                }
            }
        }

        let mut categories = Vec::new();
        for category_raw in categories_raw.into_iter() {
            let assets = cat_map.remove(&category_raw.name).unwrap();
            categories.push(Category::from_assets(
                CategoryName(category_raw.name),
                assets,
                category_raw.bound.map(|b| b.into()),
            ));
        }
        Ok(categories)
    }

    pub fn build_model(self) -> Result<(TimeRange<Year>, Model)> {
        let categories = Self::build_categories(self.plan.common.categories.clone(), self.assets)
            .context("Failed to build categories")?;

        let mut flows = self
            .flows
            .build(&self.times_table, &self.lookup_tables)
            .context("Failed to convert flows")?;

        let events = self
            .events
            .build(&self.times_table, &self.lookup_tables)
            .context("Failed to build events")?;
        for (name, event) in events.into_iter() {
            let event_flows = event
                .build_flows()
                .context(format!("Failed to build flows for event {}", name.0))?;
            for (name, flow) in event_flows {
                flows.entry(name).or_insert_with(Vec::new).push(flow);
            }
        }

        Ok((
            self.plan
                .time_range
                .try_into()
                .context("Failed to convert time range")?,
            Model::new(
                flows,
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

    let times_table = match &plan.common.times_file {
        Some(file) => load_subfile("times", plan_file, &file)?,
        None => TimesTable::default(),
    };
    let lookup_tables = match &plan.common.tables_file {
        Some(file) => LookupTables::build(load_subfile("tables", plan_file, &file)?, &times_table)
            .context("failed to build lookup tables")?,
        None => BTreeMap::new(),
    };

    Ok(Config {
        assets: load_subfile("assets", plan_file, &plan.common.assets_file)?,
        flows: load_subfile("flows", plan_file, &plan.common.flows_file)?,
        events: match &plan.common.events_file {
            Some(file) => load_subfile("events", plan_file, &file)?,
            None => Events::default(),
        },
        times_table,
        lookup_tables,
        plan,
    })
}
