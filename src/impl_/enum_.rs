#[derive(Copy, Clone, Debug)]
pub enum Enum2<A, B> {
    A(A),
    B(B),
}

#[derive(Copy, Clone, Debug)]
pub enum Enum3<A, B, C> {
    A(A),
    B(B),
    C(C),
}
