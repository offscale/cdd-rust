#[derive(Clone)]
/// Documented
pub struct ApiKey;
/// Documented
pub struct OAuth2<T>(std::marker::PhantomData<T>);
/// Documented
pub mod scopes {
    /// Documented
    pub struct WritePets;
    /// Documented
    pub struct ReadPets;
}
