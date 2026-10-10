//! Methods on your own types, called from rules, and the common Grule pattern of an
//! `Out` fact that rules report rejection reasons into.
//!
//! Run with `cargo run --example methods`.

use rust_runes::{Fact, FactValue, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Default)]
#[allow(non_snake_case)]
struct Loan {
    Amount: f64,
    PropertyValue: f64,
    MonthlyIncome: f64,
    Ltv: f64,
}

impl Loan {
    fn monthly_payment(&self, annual_rate: f64, years: f64) -> f64 {
        let r = annual_rate / 12.0;
        let n = years * 12.0;
        self.Amount * r / (1.0 - (1.0 + r).powf(-n))
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[allow(non_snake_case)]
struct Out {
    RejectionReasons: Vec<String>,
}

const RULES: &str = r#"
    rule Ltv "Compute the loan-to-value ratio" salience 100 {
        when Fact.Ltv == 0
        then Fact.Ltv = Fact.Amount / Fact.PropertyValue;
    }

    rule MaxLtv "LTV may not exceed 80%" salience 10 {
        when Fact.Ltv > 0.8
        then Out.Reject("LTV %v exceeds 80%%", Fact.Ltv); Retract("MaxLtv");
    }

    rule Affordability "Payment may not exceed 40% of income" salience 10 {
        when Fact.MonthlyPayment(0.06, 30) > Fact.MonthlyIncome * 0.4
        then Out.Reject("payment above 40%% of income"); Retract("Affordability");
    }
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = RuleEngine::new();
    engine.add_rules_from_str(RULES)?;

    // Fixed arguments decode into a tuple; the return value becomes a fact value
    engine.register_typed_method(
        "MonthlyPayment",
        |loan: &mut Loan, (rate, years): (f64, f64)| Ok(loan.monthly_payment(rate, years)),
    )?;
    // Any number of arguments decode into a Vec
    engine.register_typed_method("Reject", |out: &mut Out, args: Vec<FactValue>| {
        let mut parts = args.iter().map(|arg| match arg {
            FactValue::String(s) => s.clone(),
            FactValue::Float(f) => format!("{:.2}", f),
            other => format!("{:?}", other),
        });
        let mut reason = parts.next().unwrap_or_default();
        for part in parts {
            reason = reason.replacen("%v", &part, 1);
        }
        out.RejectionReasons.push(reason.replace("%%", "%"));
        Ok(())
    })?;

    let loan = Loan {
        Amount: 900_000.0,
        PropertyValue: 1_000_000.0,
        MonthlyIncome: 12_000.0,
        Ltv: 0.0,
    };
    let mut facts = HashMap::from([
        ("Fact".to_string(), Fact::from_serialize("Fact", &loan)?),
        (
            "Out".to_string(),
            Fact::from_serialize("Out", &Out::default())?,
        ),
    ]);
    engine.execute(&mut facts)?;

    let out: Out = facts["Out"].deserialize()?;
    println!("rejection reasons: {:#?}", out.RejectionReasons);
    Ok(())
}
