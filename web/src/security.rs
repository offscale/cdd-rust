#[derive(Clone)]
/// Documented
pub struct ApiKey;
#[derive(Clone)]
/// Documented
pub struct OAuth2<T>(std::marker::PhantomData<T>);
#[derive(Clone)]
/// Documented
pub struct PetstoreAuth<T>(std::marker::PhantomData<T>);
/// Documented
pub mod scopes {
    /// Documented
    #[derive(Clone)]
    pub struct WritePets;
    /// Documented
    #[derive(Clone)]
    pub struct ReadPets;
}
