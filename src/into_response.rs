use std::borrow::Cow;

use bytes::Bytes;

use crate::ctx::Ctx;

pub trait IntoResponse {
    fn into_response(self, c: &mut Ctx);
}

impl IntoResponse for () {
    fn into_response(self, _: &mut Ctx) {}
}

impl<T> IntoResponse for Result<T, crate::status_error::StatusError>
where
    T: IntoResponse,
{
    fn into_response(self, c: &mut Ctx) {
        match self {
            Ok(value) => value.into_response(c),
            Err(e) => {
                c.res.status(e.code);
                c.res.send(e.brief);
            }
        }
    }
}

impl<T> IntoResponse for Option<T>
where
    T: IntoResponse,
{
    fn into_response(self, c: &mut Ctx) {
        if let Some(value) = self {
            value.into_response(c)
        }
    }
}

impl IntoResponse for String {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for Vec<u8> {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for Bytes {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for &'static str {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for &'static [u8] {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for Cow<'static, str> {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}

impl IntoResponse for Cow<'static, [u8]> {
    fn into_response(self, c: &mut Ctx) {
        c.res.send(self);
    }
}
