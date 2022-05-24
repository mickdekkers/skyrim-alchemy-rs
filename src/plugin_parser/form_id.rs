pub type FormIdPair = (String, u32);

pub type FormIdPairRef<'a> = (&'a str, u32);

pub trait FormIdContainer {
    fn get_form_id_pair(&self) -> FormIdPair;
    fn get_form_id_pair_ref(&self) -> FormIdPairRef;
}
