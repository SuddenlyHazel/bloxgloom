//! Stable numeric content identities. Their widths are independent of admission limits.

macro_rules! content_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                Self(u32::from(value))
            }
        }

        impl From<u16> for $name {
            fn from(value: u16) -> Self {
                Self(u32::from(value))
            }
        }

        impl From<u32> for $name {
            fn from(value: u32) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u32 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

content_id!(BlockTypeId);
content_id!(BlockStateId);
content_id!(ItemId);
content_id!(EntityTypeId);
content_id!(TextureId);
