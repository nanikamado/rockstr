use std::fmt::Display;

pub struct LineLimit<'a>(pub &'a str);
impl Display for LineLimit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.len() > 3000 {
            write!(f, "{} ... ({} bytes)", &self.0[..3000], self.0.len())
        } else {
            write!(f, "{}", self.0)
        }
    }
}

pub enum DisplayIfSome<S: Display> {
    Some(S),
    None,
}
impl<S: Display> Display for DisplayIfSome<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            DisplayIfSome::Some(a) => write!(f, "{a}"),
            DisplayIfSome::None => Ok(()),
        }
    }
}
