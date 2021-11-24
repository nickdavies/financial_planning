use crate::tax::TaxTx;
use crate::time::Time;

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

    pub fn at_rate(&self, rate: Rate) -> Money {
        Money(self.0 * rate.0 / 10000)
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
        // We * 10000 here first * 100 because rate is a 2 decimal place value stored as an int.
        // Then we * an addition 100 because we want it as a percentage.
        //
        // Doing it this way and not using ::from_percent retains the maximum amount of precision
        Rate((self.0 * 10000) / (rhs.0))
    }
}

impl core::iter::Sum<Money> for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Self {
        Money(iter.map(|m| m.0).sum())
    }
}

/// A percentage with up to 2 decimals
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Rate(i64);

impl Rate {
    pub fn from_percent(pct: i64) -> Self {
        Self(pct * 100)
    }

    pub fn inverse(&self) -> Self {
        Self(10000 - self.0)
    }
}

impl core::ops::Div<i64> for Rate {
    type Output = Rate;
    fn div(self, rhs: i64) -> Self::Output {
        Rate(self.0 / rhs)
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
    use anyhow::Result;

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

        Ok(())
    }

    #[test]
    fn test_rate_basics() -> Result<()> {
        let r = Rate::from_percent(10);
        assert_eq!(r.0, 1000);
        let inv = r.inverse();
        assert_eq!(inv.0, 9000);
        assert_eq!(r, r.inverse().inverse());
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
        let m_out = m.at_rate(r);
        assert_eq!(m_out.as_dollars(), 20);

        // Test to prove we round well
        let m = Money::from_dollars(2);
        let r = Rate::from_percent(20);
        let m_out = m.at_rate(r);
        // externally we truncate down to 0
        assert_eq!(m_out.as_dollars(), 0);
        // Internally we should still see 40c
        assert_eq!(m_out.0, 40);

        // Next we test if we can divide money and get rates
        assert_eq!(
            Money::from_dollars(100) / Money::from_dollars(1000),
            Rate::from_percent(10),
        );
        // Test 1c as a % of $1 to test extreme end of precision
        assert_eq!(Money(1) / Money::from_dollars(1), Rate::from_percent(1),);
        // Check precision below a single % point also works
        assert_eq!(Money::from_dollars(1) / Money::from_dollars(3), Rate(3333),);

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