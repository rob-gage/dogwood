// Copyright Rob Gage 2026

use super::{MaterialForm, MaterialIdentifier};

#[test]
fn test_gas_uses_nonzero_tag_zero_identifiers_without_changing_existing_forms() {
    assert!(MaterialIdentifier::NULL.as_u32() == 0);
    assert!(MaterialIdentifier::NULL.form_checked().is_none());
    assert!(MaterialIdentifier::NULL.index() == 0);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0).as_u32() == 0x00000001);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 1).as_u32() == 0x00000002);
    assert!(
        MaterialIdentifier::new(MaterialForm::Gas, 1).form_checked() == Some(MaterialForm::Gas)
    );
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 1).index() == 1);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0x3ffffffe).as_u32() == 0x3fffffff);
    assert!(MaterialIdentifier::new(MaterialForm::Gas, 0x3ffffffe).index() == 0x3ffffffe);
    assert!(MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32() == 0x40000000);
    assert!(MaterialIdentifier::new(MaterialForm::CellularDynamic, 0).as_u32() == 0x80000000);
    assert!(MaterialIdentifier::new(MaterialForm::Fluid, 0).as_u32() == 0xc0000000);
}
