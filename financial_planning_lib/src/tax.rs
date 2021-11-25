use crate::asset::{Money, Rate};
use crate::flow::{FixedFlow, Flow, FlowName};
use crate::time::{Frequency, Month, Time, TimeNext, Year};

pub trait AnnualTaxPolicy: std::fmt::Debug {
    fn calculate_adjustment(
        &self,
        // The year in which this adjustment is for. The provided month is ignored
        year: Year,

        // A summary of the tax withheld, income earned etc
        summary: &TaxSummary,
    ) -> (TaxAdjustment, Flow) {
        let taxable_income = self.calculate_taxable_income(summary);
        let tax_owed = self.calculate_owed(taxable_income, summary);
        let delta = summary.tax_withheld - tax_owed;

        (
            TaxAdjustment {
                owed: tax_owed,
                withheld: summary.tax_withheld,
                delta,
                effective_rate: if taxable_income == Money::from_dollars(0) {
                    Rate::from_percent(0)
                } else {
                    tax_owed / taxable_income
                },
            },
            Flow {
                name: FlowName("Tax adjustment".to_string()),
                description: format!("Estimated tax refund/debt from {}", year.0),
                start: Time {
                    year: year.next(),
                    month: Month::April,
                },
                end: Time {
                    year: year.next(),
                    month: Month::May,
                },
                frequency: Frequency::Monthly,
                value: Box::new(FixedFlow { value: delta }),
                tax_policy: Box::new(TaxExempt {}),
            },
        )
    }

    fn calculate_owed(&self, taxable_income: Money, summary: &TaxSummary) -> Money;

    fn calculate_taxable_income(&self, summary: &TaxSummary) -> Money;
}

#[derive(Debug)]
pub struct FixedRateTaxPolicy {
    rate: Rate,
    deductions: Money,
}

impl FixedRateTaxPolicy {
    pub fn new(rate: Rate, deductions: Money) -> Self {
        Self { rate, deductions }
    }
}

impl AnnualTaxPolicy for FixedRateTaxPolicy {
    fn calculate_owed(&self, taxable_income: Money, _: &TaxSummary) -> Money {
        taxable_income.at_rate(self.rate)
    }

    fn calculate_taxable_income(&self, summary: &TaxSummary) -> Money {
        core::cmp::max(
            summary.taxable_income - self.deductions,
            Money::from_dollars(0),
        )
    }
}

#[derive(Debug)]
pub struct TaxAdjustment {
    pub owed: Money,
    pub withheld: Money,
    pub delta: Money,
    pub effective_rate: Rate,
}

#[derive(Debug)]
pub struct TaxSummary {
    pub net_amount: Money,
    pub taxable_income: Money,
    pub tax_withheld: Money,
}

impl TaxSummary {
    pub fn new() -> Self {
        Self {
            net_amount: Money::from_dollars(0),
            taxable_income: Money::from_dollars(0),
            tax_withheld: Money::from_dollars(0),
        }
    }

    pub fn apply_tx(&mut self, tx: &TaxTx, net: Money) {
        self.taxable_income = self.taxable_income + tx.taxable_income;
        self.tax_withheld = self.tax_withheld + tx.tax_withheld;
        self.net_amount = self.net_amount + net;
    }
}

#[derive(Debug, Clone)]
pub struct TaxTx {
    pub taxable_income: Money,
    pub tax_withheld: Money,
}

pub trait TaxPolicy: std::fmt::Debug {
    fn calculate_tax(&self, gross: Money) -> (Money, TaxTx) {
        let tx = self.tax_withheld(gross);

        (gross - tx.tax_withheld, tx)
    }

    fn tax_withheld(&self, gross: Money) -> TaxTx;
}

#[derive(Debug)]
pub struct NoWithholding {}
impl TaxPolicy for NoWithholding {
    fn tax_withheld(&self, gross: Money) -> TaxTx {
        TaxTx {
            taxable_income: gross,
            tax_withheld: Money::from_dollars(0),
        }
    }
}

#[derive(Debug)]
pub struct TaxExempt {}
impl TaxPolicy for TaxExempt {
    fn tax_withheld(&self, _: Money) -> TaxTx {
        TaxTx {
            taxable_income: Money::from_dollars(0),
            tax_withheld: Money::from_dollars(0),
        }
    }
}

#[derive(Debug)]
pub struct ConstantTaxPolicy {
    pub rate: Rate,
}

impl TaxPolicy for ConstantTaxPolicy {
    fn tax_withheld(&self, gross: Money) -> TaxTx {
        TaxTx {
            taxable_income: gross,
            tax_withheld: gross.at_rate(self.rate),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;

    use crate::asset::{Category, CategoryName};

    fn verify_tax_adjustment(
        adjustment: &TaxAdjustment,
        flow: &Flow,
        year: Year,
        owed: Money,
        withheld: Money,
        delta: Money,
        rate: Rate,
    ) -> Result<()> {
        assert_eq!(adjustment.owed, owed);
        assert_eq!(adjustment.withheld, withheld);

        // Delta is owed vs withheld (+ is a refund)
        assert_eq!(adjustment.delta, delta);

        // Rate is our taxable income vs owed
        assert_eq!(adjustment.effective_rate, rate);

        // We must make sure the flow is once-off and next year
        assert_eq!(flow.start.year, year.next());
        assert_eq!(flow.start.next(), flow.end);
        assert_eq!(flow.frequency, Frequency::Monthly);

        // We make sure that any flow made by tax is not taxed
        let tx = flow.tax_policy.tax_withheld(delta);
        assert_eq!(tx.taxable_income, Money::from_dollars(0));
        assert_eq!(tx.tax_withheld, Money::from_dollars(0));

        let (net, tx) = flow.tax_policy.calculate_tax(delta);
        assert_eq!(net, delta);
        assert_eq!(tx.taxable_income, Money::from_dollars(0));
        assert_eq!(tx.tax_withheld, Money::from_dollars(0));

        // Finally we make sure that the value is set correctly
        assert!(flow.value.applies_at(&flow.start, &flow));
        assert_eq!(
            flow.value.value_at(
                &flow.start,
                &flow,
                &Category::from_assets(CategoryName("unittest".to_string()), vec![]).value(),
            ),
            delta,
        );

        Ok(())
    }

    #[test]
    fn test_annual_generic() -> Result<()> {
        #[derive(Debug)]
        struct Test {}
        impl AnnualTaxPolicy for Test {
            fn calculate_owed(&self, _: Money, _: &TaxSummary) -> Money {
                Money::from_dollars(500)
            }

            fn calculate_taxable_income(&self, _: &TaxSummary) -> Money {
                Money::from_dollars(1000)
            }
        }

        let (adjustment, flow) = Test {}.calculate_adjustment(
            Year(2021),
            &TaxSummary {
                net_amount: Money::from_dollars(2000),
                taxable_income: Money::from_dollars(3000),
                tax_withheld: Money::from_dollars(600),
            },
        );

        verify_tax_adjustment(
            &adjustment,
            &flow,
            Year(2021),
            // Owed is hard coded above
            Money::from_dollars(500),
            // Withheld should be proxied through unchanged
            Money::from_dollars(600),
            // Delta is owed vs withheld (+ is a refund)
            Money::from_dollars(100),
            // Rate is our taxable income vs owed (500 vs 10000)
            // (the income in the summary is given to the struct but we ignore it)
            Rate::from_percent(50),
        )
    }

    #[test]
    fn test_fixed_annual() -> Result<()> {
        let p = FixedRateTaxPolicy::new(Rate::from_percent(20), Money::from_dollars(1000));

        let (adjustment, flow) = p.calculate_adjustment(
            Year(2021),
            &TaxSummary {
                net_amount: Money::from_dollars(5000),
                taxable_income: Money::from_dollars(10000),
                tax_withheld: Money::from_dollars(3000),
            },
        );

        verify_tax_adjustment(
            &adjustment,
            &flow,
            Year(2021),
            // Tax owed should be:
            //    owed = (taxable_income - deductions) * tax rate
            //    1800 = (10000 - 1000) * 20%
            Money::from_dollars(1800),
            // Withheld should be proxied through unchanged
            Money::from_dollars(3000),
            // Delta is owed vs withheld (+ is a refund)
            Money::from_dollars(1200),
            // Rate should be calculated back to the same
            Rate::from_percent(20),
        )
    }

    #[test]
    fn test_tax_summary() -> Result<()> {
        let mut s = TaxSummary::new();

        s.apply_tx(
            &TaxTx {
                taxable_income: Money::from_dollars(100),
                tax_withheld: Money::from_dollars(10),
            },
            Money::from_dollars(1000),
        );
        s.apply_tx(
            &TaxTx {
                taxable_income: Money::from_dollars(200),
                tax_withheld: Money::from_dollars(20),
            },
            Money::from_dollars(2000),
        );

        assert_eq!(s.net_amount, Money::from_dollars(3000));
        assert_eq!(s.taxable_income, Money::from_dollars(300));
        assert_eq!(s.tax_withheld, Money::from_dollars(30));

        Ok(())
    }

    fn test_tax_policy<P: TaxPolicy>(
        policy: P,
        gross: Money,
        taxable: Money,
        withheld: Money,
        net: Money,
    ) -> Result<()> {
        let tx = policy.tax_withheld(gross);
        assert_eq!(tx.taxable_income, taxable);
        assert_eq!(tx.tax_withheld, withheld);

        let (net_out, tx) = policy.calculate_tax(gross);
        assert_eq!(tx.taxable_income, taxable);
        assert_eq!(tx.tax_withheld, withheld);
        assert_eq!(net_out, net);

        let (net_out, tx) = policy.calculate_tax(Money::from_dollars(0));
        assert_eq!(tx.taxable_income, Money::from_dollars(0));
        assert_eq!(tx.tax_withheld, Money::from_dollars(0));
        assert_eq!(net_out, Money::from_dollars(0));

        Ok(())
    }

    #[test]
    fn test_no_withholding() -> Result<()> {
        test_tax_policy(
            NoWithholding {},
            Money::from_dollars(1000), // gross
            Money::from_dollars(1000), // taxable
            Money::from_dollars(0),    // withheld
            Money::from_dollars(1000), // net
        )
    }

    #[test]
    fn test_tax_exempt() -> Result<()> {
        test_tax_policy(
            TaxExempt {},
            Money::from_dollars(2000), // gross
            Money::from_dollars(0),    // taxable
            Money::from_dollars(0),    // withheld
            Money::from_dollars(2000), // net
        )
    }

    #[test]
    fn test_constant_tax() -> Result<()> {
        test_tax_policy(
            ConstantTaxPolicy {
                rate: Rate::from_percent(25),
            },
            Money::from_dollars(1000), // gross
            Money::from_dollars(1000), // taxable
            Money::from_dollars(250),  // withheld
            Money::from_dollars(750),  // net
        )
    }
}
