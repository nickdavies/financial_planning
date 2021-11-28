use crate::tax::TaxTx;
use crate::time::Time;

use anyhow::{anyhow, Context, Result};
use thousands::Separable;

/// An amount of money in cents
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Money(i64);

impl Money {
    pub fn from_dollars(amount: i64) -> Self {
        Self(amount * 100)
    }

    pub fn from_cents(amount: i64) -> Self {
        Self(amount)
    }

    pub fn as_dollars(self) -> i64 {
        self.0 / 100
    }

    pub fn as_cents(self) -> i64 {
        self.0
    }

    pub fn at_rate(&self, rate: Rate) -> Result<Money> {
        rate.at_rate(*self)
    }

    pub fn negate(&self) -> Self {
        Money(self.0 * -1)
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let cents = self.as_cents();
        let remainder = cents % 100;
        write!(
            f,
            "${}{}",
            self.as_dollars().separate_with_commas(),
            if remainder != 0 {
                format!(".{:02}", remainder)
            } else {
                "".to_string()
            }
        )
    }
}

impl core::ops::Sub for Money {
    type Output = Money;

    fn sub(self, rhs: Self) -> Self::Output {
        Money(self.0 - rhs.0)
    }
}
impl core::ops::Add for Money {
    type Output = Money;

    fn add(self, rhs: Self) -> Self::Output {
        Money(self.0 + rhs.0)
    }
}

impl core::ops::Div for Money {
    type Output = Rate;

    fn div(self, rhs: Self) -> Self::Output {
        // We construct the Rate first for the LHS because it will then internally store it with
        // the max precision we allow. Then when we divide by the RHS then we will lose as little
        // detail as possible.
        //
        // If we divide the money first and then make it a rate we will usually round away huge
        // amounts of precision and cause massive errors in the model.
        Rate::from_percent(self.0 * 100) / rhs.0
    }
}

impl core::iter::Sum<Money> for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Self {
        Money(iter.map(|m| m.0).sum())
    }
}

// The internal conversion ratio of rate. Used to scale the number of decimal places supported.
// The tradeoff between more precision is that you will have more overflows when performing rate
// calculations.
const RATE_PRECISION: u32 = 6;
const RATE_SCALE: i64 = (10 as i64).pow(RATE_PRECISION);

/// A percentage with a fixed amount of decimal places
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Rate(i64);

impl Rate {
    pub fn from_percent(pct: i64) -> Self {
        Self(pct * RATE_SCALE)
    }

    pub fn as_percent(&self) -> i64 {
        self.0 / RATE_SCALE
    }

    pub fn inverse(&self) -> Self {
        Rate::from_percent(100) - *self
    }

    pub fn negate(&self) -> Self {
        Rate(self.0 * -1)
    }

    pub fn at_rate(&self, money: Money) -> Result<Money> {
        let tmp: i64 = money
            .0
            .checked_mul(self.0)
            .context("Applying rate would cause overflow")?;
        Ok(Money(tmp / RATE_SCALE / 100))
    }

    pub(crate) fn to_float(&self) -> f64 {
        self.0 as f64 / RATE_SCALE as f64 / 100.0
    }

    pub(crate) fn from_float(other: f64) -> Self {
        Rate((other * 100.0 * RATE_SCALE as f64) as i64)
    }
}

impl core::ops::Sub<Rate> for Rate {
    type Output = Rate;
    fn sub(self, rhs: Self) -> Self::Output {
        Rate(self.0 - rhs.0)
    }
}

impl core::ops::Div<i64> for Rate {
    type Output = Rate;
    fn div(self, rhs: i64) -> Self::Output {
        Rate(self.0 / rhs)
    }
}

impl std::str::FromStr for Rate {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let clean = s.trim().trim_end_matches('%').trim();

        Ok(match clean.split_once('.') {
            Some((whole_str, points_str)) => {
                let _: f64 = clean.parse()?;
                let points: i64 = points_str.parse()?;
                if points >= RATE_SCALE {
                    return Err(anyhow!(
                        "Found more than {} decimal places for {} which isn't allowed",
                        RATE_PRECISION,
                        s
                    ));
                }
                if points < 0 {
                    return Err(anyhow!(
                        "Found negative number on right side of . somehow for {}",
                        s
                    ));
                }

                let digits = points_str.len() as u32;
                let whole: i64 = whole_str.parse()?;
                Rate(whole * RATE_SCALE + points * (10 as i64).pow(RATE_PRECISION - digits))
            }
            None => Rate::from_percent(clean.parse()?),
        })
    }
}

impl std::fmt::Display for Rate {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let remainder = self.0 % RATE_SCALE;
        write!(
            f,
            "{}{}%",
            self.0 / RATE_SCALE,
            if remainder != 0 {
                format!(".{:02}", remainder)
            } else {
                "".to_string()
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct AssetName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Asset {
    pub name: AssetName,
    pub value: Money,
}

#[derive(Debug, Clone)]
pub struct Tx {
    pub time: Time,
    pub amount: Money,
    pub tax_tx: TaxTx,
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct CategoryName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Category {
    pub name: CategoryName,
    pub assets: Vec<Asset>,
}

impl Category {
    pub fn from_assets(name: CategoryName, assets: Vec<Asset>) -> Self {
        Category { name, assets }
    }

    pub fn value<'a>(&'a self) -> CategoryValue<'a> {
        CategoryValue(self, self.assets.iter().map(|a| a.value).sum())
    }
}

pub struct CategoryValue<'a>(&'a Category, Money);

impl<'a> CategoryValue<'a> {
    pub fn name(&self) -> &CategoryName {
        &self.0.name
    }

    pub fn value(&self) -> Money {
        self.1
    }

    pub fn apply_tx(&mut self, tx: &Tx) {
        self.1 = self.1 + tx.amount;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{Context, Result};

    use crate::time::{Month, Year};

    #[test]
    fn test_money_basics() -> Result<()> {
        let m = Money::from_dollars(1000000);
        assert_eq!(m.as_dollars(), 1000000);
        assert_eq!(format!("{}", m), "$1,000,000");

        let m = Money::from_cents(123456);
        assert_eq!(m.as_dollars(), 1234);
        assert_eq!(format!("{}", m), "$1,234.56");

        assert_eq!(Money::from_cents(100), Money::from_dollars(1));
        assert_ne!(Money::from_cents(101), Money::from_dollars(1));

        assert_eq!(Money::from_cents(101).as_cents(), 101);
        assert_eq!(Money::from_dollars(1).as_cents(), 100);
        Ok(())
    }

    #[test]
    fn test_money_ops() -> Result<()> {
        let m1 = Money::from_dollars(10);
        let m2 = Money::from_dollars(10);
        let m3 = Money::from_dollars(5);

        assert_eq!(m1, m2);
        assert!(m1 > m3);
        assert_eq!((m1 + m3).as_dollars(), 15);
        assert_eq!((m1 - m3).as_dollars(), 5);
        assert_eq!((m3 - m1).as_dollars(), -5);

        let total: Money = vec![m1, m2, m3].into_iter().sum();
        assert_eq!(total, Money::from_dollars(25));

        assert_eq!(m1.negate().as_dollars(), -10);
        assert_eq!(m1.negate().negate(), m1);
        assert_eq!(m1.negate().negate().as_cents(), m1.as_cents());

        Ok(())
    }

    #[test]
    fn test_rate_basics() -> Result<()> {
        let r = Rate::from_percent(10);
        assert_eq!(r.as_percent(), 10);
        assert_eq!(r.negate().as_percent(), -10);
        assert_eq!("10%".to_string(), format!("{}", r));
        assert_eq!("-10%".to_string(), format!("{}", r.negate()));

        let inv = r.inverse();
        assert_eq!(inv.as_percent(), 90);
        assert_eq!("90%".to_string(), format!("{}", inv));
        assert_eq!(r, r.inverse().inverse());

        let r = Rate(12345678);
        assert_eq!(r.as_percent(), 12);
        assert_eq!("12.345678%".to_string(), format!("{}", r));

        Ok(())
    }

    #[test]
    fn test_rate_loading() -> Result<()> {
        let values = vec![
            ("1", 1000000),
            ("1.1", 1100000),
            ("1.01", 1010000),
            ("1.001", 1001000),
            ("1.0001", 1000100),
            ("100.51", 100510000),
            ("10%", 10000000),
            (" 10%", 10000000),
            ("10% ", 10000000),
            (" 10% ", 10000000),
            (" 10 % ", 10000000),
            (" -10 % ", -10000000),
        ];

        for (input, output) in values.into_iter() {
            let r: Rate = input
                .parse()
                .context(format!("Failed to parse {}", input))?;
            assert_eq!((input, r.0), (input, output));
        }

        let bad_values = vec![
            "a",
            "a.b",
            "0.a",
            "0a",
            "0.0a",
            "0a.0",
            "0%.0",
            "- 0", // must be touching number
            "0.-1",
            "1.1000000", // don't support more than 6 decimal places for now.
            "1.1234567", // don't support more than 6 decimal places for now.
        ];
        for input in bad_values.into_iter() {
            let r: Result<Rate> = input.parse();
            assert_eq!((input, r.is_err()), (input, true));
        }

        Ok(())
    }

    #[test]
    fn test_rate_ops() -> Result<()> {
        let r1 = Rate::from_percent(20);
        let r2 = Rate::from_percent(20);
        let r3 = Rate::from_percent(10);

        assert_eq!(r1, r2);
        assert!(r1 > r3);
        assert_eq!(r1 / 10, Rate::from_percent(2));

        Ok(())
    }

    #[test]
    fn test_rate_money_ops() -> Result<()> {
        // Test without rounding issues
        let m = Money::from_dollars(100);
        let r = Rate::from_percent(20);
        let m_out = m.at_rate(r).unwrap();
        assert_eq!(m_out.as_dollars(), 20);

        let m_out = r.at_rate(m).unwrap();
        assert_eq!(m_out.as_dollars(), 20);

        // Test to prove we round well
        let m = Money::from_dollars(2);
        let r = Rate::from_percent(20);
        let m_out = m.at_rate(r).unwrap();
        // externally we truncate down to 0
        assert_eq!(m_out.as_dollars(), 0);
        // Internally we should still see 40c
        assert_eq!(m_out.0, 40);

        let m_out = r.at_rate(m).unwrap();
        // externally we truncate down to 0
        assert_eq!(m_out.as_dollars(), 0);
        // Internally we should still see 40c
        assert_eq!(m_out.0, 40);

        // Next test at_rate with tiny rates to make sure we don't
        // truncate where we shouldn't.
        let r = Rate::from_percent(1) / 10;
        let m = Money::from_dollars(2000);
        assert_eq!(m.at_rate(r).unwrap(), Money::from_dollars(2));
        assert_eq!(r.at_rate(m).unwrap(), Money::from_dollars(2));

        let m = Money::from_dollars(20);
        assert_eq!(m.at_rate(r).unwrap(), Money::from_cents(2));
        assert_eq!(r.at_rate(m).unwrap(), Money::from_cents(2));

        // Next we test if we can divide money and get rates
        assert_eq!(
            Money::from_dollars(100) / Money::from_dollars(1000),
            Rate::from_percent(10),
        );
        // Test 1c as a % of $1 to test extreme end of precision
        assert_eq!(Money(1) / Money::from_dollars(1), Rate::from_percent(1),);
        // Check precision below a single % point also works
        assert_eq!(
            Money::from_dollars(1) / Money::from_dollars(3),
            Rate(33333333)
        );

        Ok(())
    }

    #[test]
    fn test_category_basics() -> Result<()> {
        let c = Category::from_assets(CategoryName("test1".to_string()), vec![]);

        assert_eq!(c.name, CategoryName("test1".to_string()));
        assert!(c.assets.is_empty());

        let val = c.value();
        assert_eq!(val.0.name.0, "test1".to_string());
        assert_eq!(val.1, Money::from_dollars(0));

        let assets = vec![
            Asset {
                name: AssetName("a1".to_string()),
                value: Money::from_dollars(100),
            },
            Asset {
                name: AssetName("a2".to_string()),
                value: Money::from_dollars(50),
            },
            Asset {
                name: AssetName("a3".to_string()),
                value: Money::from_dollars(-200),
            },
        ];

        let c = Category::from_assets(CategoryName("test2".to_string()), assets.clone());
        assert_eq!(c.name, CategoryName("test2".to_string()));
        assert_eq!(c.assets, assets);

        let val = c.value();
        assert_eq!(val.0.name.0, "test2".to_string());
        assert_eq!(val.1, Money::from_dollars(-50));

        Ok(())
    }

    #[test]
    fn test_category_value() -> Result<()> {
        let assets = vec![
            Asset {
                name: AssetName("a1".to_string()),
                value: Money::from_dollars(100),
            },
            Asset {
                name: AssetName("a2".to_string()),
                value: Money::from_dollars(50),
            },
            Asset {
                name: AssetName("a3".to_string()),
                value: Money::from_dollars(-200),
            },
        ];

        let c = Category::from_assets(CategoryName("test2".to_string()), assets.clone());
        assert_eq!(c.name, CategoryName("test2".to_string()));
        assert_eq!(c.assets, assets);

        let mut val = c.value();

        assert_eq!(val.name().0, "test2".to_string());
        assert_eq!(val.value(), Money::from_dollars(-50));

        val.apply_tx(&Tx {
            time: Time {
                year: Year(2021),
                month: Month::January,
            },
            amount: Money::from_dollars(80),
            tax_tx: TaxTx {
                taxable_income: Money::from_dollars(123),
                tax_withheld: Money::from_dollars(456),
            },
        });
        assert_eq!(val.value(), Money::from_dollars(30));

        Ok(())
    }
}
