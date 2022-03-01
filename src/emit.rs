pub fn field() -> Box<dyn std::any::Any> {
    todo!()
}

pub fn diesel_model() -> Box<dyn std::any::Any> {
    todo!()
}

pub fn route() -> Box<dyn std::any::Any> {
    todo!()
}

pub fn test() -> Box<dyn std::any::Any> {
    todo!()
}

pub fn request_mock() -> Box<dyn std::any::Any> {
    todo!()
}

pub fn response_mock() -> Box<dyn std::any::Any> {
    todo!()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_emits_route() {
        // Add simple mock here, plus parse from—not import—file in actix_web_mocks dir
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
