//! Macro for implementing cyclic navigation on enums.

/// Generates `ALL`, `index`, `from_index`, `next`, and `prev` methods for
/// enums that represent a fixed set of cyclically-navigable variants.
macro_rules! cyclic_enum {
    ($Enum:ident { $($Variant:ident => $idx:literal),+ $(,)? }) => {
        impl $Enum {
            pub const ALL: [Self; cyclic_enum!(@count $($Variant),+)] = [$(Self::$Variant),+];

            pub fn index(self) -> usize {
                match self {
                    $(Self::$Variant => $idx,)+
                }
            }

            pub fn from_index(index: usize) -> Self {
                match index {
                    $($idx => Self::$Variant,)+
                    _ => Self::ALL[0],
                }
            }

            pub fn next(self) -> Self {
                Self::from_index((self.index() + 1) % Self::ALL.len())
            }

            pub fn prev(self) -> Self {
                let len = Self::ALL.len();
                Self::from_index((self.index() + len - 1) % len)
            }
        }
    };
    (@count $head:ident $(,$tail:ident)*) => {
        1 + cyclic_enum!(@count $($tail),*)
    };
    (@count) => {
        0
    };
}
