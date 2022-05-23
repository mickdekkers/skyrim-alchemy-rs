pub type FormIdPair = (String, u32);

pub trait FormIdContainer {
    fn get_form_id_pair(&self) -> FormIdPair;
}
