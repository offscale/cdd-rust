#[derive(Clone)]
pub struct ApiKey;
pub struct OAuth2<T>(std::marker::PhantomData<T>);
pub mod scopes {
    pub struct WritePets;
    pub struct ReadPets;
}
