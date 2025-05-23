use rust_runes::ast;
use rust_runes::*;
use std::collections::HashMap;

fn main() -> Result<()> {
    // Create a rule engine
    let mut engine = RuleEngine::new();

    // Create some example rules using the programmatic API
    let speed_up_rule = Rule::new(
        "SpeedUp".to_string(),
        10,
        ast::Expression::And(
            Box::new(ast::Expression::Equal(
                Box::new(ast::Expression::FieldAccess(
                    Box::new(ast::Expression::Variable("TestCar".to_string())),
                    "SpeedUp".to_string(),
                )),
                Box::new(ast::Expression::Boolean(true)),
            )),
            Box::new(ast::Expression::LessThan(
                Box::new(ast::Expression::FieldAccess(
                    Box::new(ast::Expression::Variable("TestCar".to_string())),
                    "Speed".to_string(),
                )),
                Box::new(ast::Expression::FieldAccess(
                    Box::new(ast::Expression::Variable("TestCar".to_string())),
                    "MaxSpeed".to_string(),
                )),
            )),
        ),
        vec![
            ast::Expression::FieldAssignment(
                "TestCar".to_string(),
                "Speed".to_string(),
                Box::new(ast::Expression::Add(
                    Box::new(ast::Expression::FieldAccess(
                        Box::new(ast::Expression::Variable("TestCar".to_string())),
                        "Speed".to_string(),
                    )),
                    Box::new(ast::Expression::FieldAccess(
                        Box::new(ast::Expression::Variable("TestCar".to_string())),
                        "SpeedIncrement".to_string(),
                    )),
                )),
            ),
            ast::Expression::FieldAssignment(
                "DistanceRecord".to_string(),
                "TotalDistance".to_string(),
                Box::new(ast::Expression::Add(
                    Box::new(ast::Expression::FieldAccess(
                        Box::new(ast::Expression::Variable("DistanceRecord".to_string())),
                        "TotalDistance".to_string(),
                    )),
                    Box::new(ast::Expression::FieldAccess(
                        Box::new(ast::Expression::Variable("TestCar".to_string())),
                        "Speed".to_string(),
                    )),
                )),
            ),
        ],
    )
    .with_description("When testcar is speeding up we keep increase the speed.".to_string());

    engine.add_rule(speed_up_rule)?;

    // Create facts
    let mut facts = HashMap::new();

    let mut test_car_fields = HashMap::new();
    test_car_fields.insert("SpeedUp".to_string(), FactValue::Boolean(true));
    test_car_fields.insert("Speed".to_string(), FactValue::Number(50.0));
    test_car_fields.insert("MaxSpeed".to_string(), FactValue::Number(100.0));
    test_car_fields.insert("SpeedIncrement".to_string(), FactValue::Number(10.0));

    let mut distance_record_fields = HashMap::new();
    distance_record_fields.insert("TotalDistance".to_string(), FactValue::Number(0.0));

    facts.insert(
        "TestCar".to_string(),
        Fact::from_object("TestCar".to_string(), test_car_fields),
    );
    facts.insert(
        "DistanceRecord".to_string(),
        Fact::from_object("DistanceRecord".to_string(), distance_record_fields),
    );

    // Execute rules
    println!("Before execution:");
    println!(
        "TestCar.Speed: {:?}",
        facts.get("TestCar").unwrap().get_field("Speed")
    );
    println!(
        "DistanceRecord.TotalDistance: {:?}",
        facts
            .get("DistanceRecord")
            .unwrap()
            .get_field("TotalDistance")
    );

    let result = engine.execute(&mut facts)?;

    println!("\nAfter execution:");
    println!(
        "TestCar.Speed: {:?}",
        facts.get("TestCar").unwrap().get_field("Speed")
    );
    println!(
        "DistanceRecord.TotalDistance: {:?}",
        facts
            .get("DistanceRecord")
            .unwrap()
            .get_field("TotalDistance")
    );
    println!("Rules fired: {:?}", result.rules_fired);
    println!("Execution time: {}ms", result.execution_time_ms);

    // Example of parsing GRL syntax
    let grl_text = r#"
        rule SpeedUp "When testcar is speeding up we keep increase the speed." salience 10 {
            when
                TestCar.SpeedUp == true && TestCar.Speed < TestCar.MaxSpeed
            then
                TestCar.Speed = TestCar.Speed + TestCar.SpeedIncrement;
                DistanceRecord.TotalDistance = DistanceRecord.TotalDistance + TestCar.Speed;
        }
    "#;

    let parser = parser::GrlParser::new();
    match parser.parse_rule(grl_text) {
        Ok(parsed_rule) => {
            println!("\nSuccessfully parsed GRL rule: {}", parsed_rule.name);
            println!("Description: {:?}", parsed_rule.description);
            println!("Salience: {}", parsed_rule.salience);
        }
        Err(e) => {
            println!("Failed to parse GRL: {}", e);
        }
    }

    Ok(())
}
