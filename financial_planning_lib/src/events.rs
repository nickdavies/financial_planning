use anyhow::{Context, Result};

use crate::asset::{CategoryName, Money, Rate};
use crate::flow::{FixedFlow, Flow, FlowName, RateFlow};
use crate::tax::TaxExempt;
use crate::time::{Frequency, Time, TimeNext, TimeRange};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct EventName(pub String);

pub trait BuildFlows {
    fn build_flows(&self) -> Result<Vec<(CategoryName, Flow)>>;
}

pub struct HousePurchase {
    // The name of the property
    pub property_name: String,

    // The time range for the whole mortgage starting from the
    // purchase date
    pub time_range: TimeRange<Time>,

    // The rate of the mortgage
    pub mortgage_rate: Rate,

    // The total value of the house at purchase time.
    pub purchase_price: Money,

    // Non-refundable costs for setting up the mortgage. This comes
    // out of the same down_payment_category.
    pub setup_cost: Money,

    // The total down-payment. This ends up in the equity category and
    // the mortgage value starts as the purchase_price - down_payment
    pub down_payment: Money,

    // The category used to track the equity in the house
    pub house_value_category: CategoryName,

    // The category where the mortgage debt will be tracked
    pub mortgage_category: CategoryName,

    // The downpayment and regular payment categories respectively.
    pub down_payment_category: CategoryName,
    pub regular_payment_category: CategoryName,
}

impl HousePurchase {
    fn start_tx(
        &self,
        name: FlowName,
        description: String,
        category_name: CategoryName,
        value: Money,
    ) -> (CategoryName, Flow) {
        (
            category_name.clone(),
            Flow {
                name,
                description,
                start: self.time_range.start.clone(),
                end: self.time_range.start.next(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(FixedFlow { value }),
            },
        )
    }

    fn calculate_repayment(
        loan: Money,
        term: &TimeRange<Time>,
        annual_rate: Rate,
    ) -> Result<Money> {
        let months = &term.end - &term.start;
        let monthly_rate = annual_rate / 12;

        let ratef = monthly_rate.to_float();
        let numerator = (1.0 + ratef).powi(months.0 as i32);
        let denominator = numerator - 1.0;
        let monthly_rate = ratef * (numerator / denominator);

        loan.at_rate(Rate::from_float(monthly_rate))
            .context("Failed to scale final result to monthly rate")
    }
}

pub fn make_transaction(
    name: String,
    source: CategoryName,
    target: CategoryName,
    time: Time,
    value: Money,
) -> Vec<(CategoryName, Flow)> {
    vec![
        (
            source.clone(),
            Flow {
                name: FlowName(format!("{} source", name)),
                description: format!(
                    "Source side of once off transfer from {} to {}",
                    source.0, target.0
                ),
                start: time.clone(),
                end: time.clone(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(FixedFlow {
                    value: Money::from_cents(value.as_cents() * -1),
                }),
            },
        ),
        (
            target.clone(),
            Flow {
                name: FlowName(format!("{} target", name)),
                description: format!(
                    "Target side of once off transfer from {} to {}",
                    source.0, target.0
                ),
                start: time.clone(),
                end: time.clone(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(FixedFlow { value }),
            },
        ),
    ]
}

impl BuildFlows for HousePurchase {
    fn build_flows(&self) -> Result<Vec<(CategoryName, Flow)>> {
        // Mortgage is the following setup transactions:
        //  house_value_category += purchase_price
        //  down_payment_category -= down_payment
        //  mortgage_category -= (purchase_price - down_payment)
        //
        let mut out = Vec::new();

        let loan_value = self.purchase_price.negate() + self.down_payment;
        out.push(self.start_tx(
            FlowName(format!("{} initial mortgage setup", self.property_name)),
            format!(
                "The initial setup of the mortgage for {}",
                self.property_name
            ),
            self.mortgage_category.clone(),
            loan_value,
        ));

        out.push(self.start_tx(
            FlowName(format!("{} initial house value", self.property_name)),
            format!(
                "The initial purchase price of the house {}",
                self.property_name
            ),
            self.house_value_category.clone(),
            self.purchase_price,
        ));

        out.push(self.start_tx(
            FlowName(format!("{} down payment", self.property_name)),
            format!("Down payment for house {}", self.property_name),
            self.down_payment_category.clone(),
            self.down_payment.negate(),
        ));

        out.push(self.start_tx(
            FlowName(format!("{} mortgage setup cost", self.property_name)),
            format!(
                "Costs involved with creating the mortgage {}",
                self.property_name
            ),
            self.down_payment_category.clone(),
            self.setup_cost.negate(),
        ));

        let payment = Self::calculate_repayment(
            self.purchase_price - self.down_payment,
            &self.time_range,
            self.mortgage_rate,
        )
        .context("Failed to calculate mortgage repayment")?;

        out.push((
            self.regular_payment_category.clone(),
            Flow {
                name: FlowName(format!("{} loan payment", self.property_name)),
                description: format!(
                    "The regular repayments for the loan on {}",
                    self.property_name
                ),
                start: self.time_range.start.next(),
                end: self.time_range.end.next(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(FixedFlow { value: payment }),
            },
        ));

        out.push((
            self.mortgage_category.clone(),
            Flow {
                name: FlowName(format!("{} loan payment", self.property_name)),
                description: format!(
                    "The regular repayments for the loan on {}",
                    self.property_name
                ),
                start: self.time_range.start.next(),
                end: self.time_range.end.next(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(FixedFlow { value: payment }),
            },
        ));

        out.push((
            self.mortgage_category.clone(),
            Flow {
                name: FlowName(format!("{} mortgage interest", self.property_name)),
                description: format!(
                    "The regular interest costs for the loan on {}",
                    self.property_name
                ),
                start: self.time_range.start.next(),
                end: self.time_range.end.next(),
                frequency: Frequency::Monthly,
                tax_policy: Box::new(TaxExempt {}),
                value: Box::new(RateFlow {
                    rate: self.mortgage_rate / 12,
                }),
            },
        ));

        Ok(out)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::time::{Month, Year};

    #[test]
    fn test_calculate_repayments() -> Result<()> {
        assert_eq!(
            HousePurchase::calculate_repayment(
                Money::from_dollars(200000),
                &TimeRange {
                    start: Time {
                        year: Year(0),
                        month: Month::January
                    },
                    end: Time {
                        year: Year(30),
                        month: Month::January
                    },
                },
                "6.5%".parse().unwrap(),
            )
            .unwrap(),
            Money::from_cents(126413),
        );

        // An extremely large mortgage and a small rate stretches the
        // limits on our precision.
        assert_eq!(
            HousePurchase::calculate_repayment(
                Money::from_dollars(10000000),
                &TimeRange {
                    start: Time {
                        year: Year(0),
                        month: Month::January
                    },
                    end: Time {
                        year: Year(30),
                        month: Month::January
                    },
                },
                "0.1%".parse().unwrap(),
            )
            .unwrap()
            .as_dollars(),
            28197,
        );

        Ok(())
    }
}
