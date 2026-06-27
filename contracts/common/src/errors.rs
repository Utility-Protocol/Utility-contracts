use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ArithmeticError {
    Overflow = 100,
    Underflow = 101,
    DivisionByZero = 102,
}
