//! Platform request transitions, independent of controls and native handles.
use crate::ApplicationError;
use zui_platform::ImeRequest;
use zui_types::Rect;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Command {
    Enabled(bool),
    CursorArea(Rect),
}
#[derive(Default)]
pub(crate) struct ImeSync {
    current: Option<ImeRequest>,
}
impl ImeSync {
    pub(crate) fn sync(
        &mut self,
        desired: Option<ImeRequest>,
        mut emit: impl FnMut(Command),
    ) -> Result<(), ApplicationError> {
        if desired.is_some_and(|r| !r.cursor_area.is_valid() || r.cursor_area.size.height <= 0.0) {
            if self.current.take().is_some() {
                emit(Command::Enabled(false));
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid IME cursor area",
            )
            .into());
        }
        let changed_session = self.current.map(|r| r.session) != desired.map(|r| r.session);
        if changed_session && self.current.is_some() {
            emit(Command::Enabled(false));
        }
        if let Some(request) = desired {
            if changed_session
                || self
                    .current
                    .is_none_or(|old| old.cursor_area != request.cursor_area)
            {
                emit(Command::CursorArea(request.cursor_area));
            }
            if changed_session {
                emit(Command::Enabled(true));
            }
        }
        self.current = desired;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zui_platform::ImeSessionId;
    #[test]
    fn transitions_reset_sessions_and_update_geometry_without_restarting() {
        let mut sync = ImeSync::default();
        let mut log = Vec::new();
        let mut request = ImeRequest {
            session: ImeSessionId {
                owner: 1,
                generation: 0,
            },
            cursor_area: Rect::new(10.0, 20.0, 1.0, 24.0),
        };
        sync.sync(Some(request), |c| log.push(c)).unwrap();
        assert_eq!(
            log,
            [
                Command::CursorArea(request.cursor_area),
                Command::Enabled(true)
            ]
        );
        log.clear();
        sync.sync(Some(request), |c| log.push(c)).unwrap();
        assert!(log.is_empty());
        request.cursor_area.origin.x += 5.0;
        sync.sync(Some(request), |c| log.push(c)).unwrap();
        assert_eq!(log, [Command::CursorArea(request.cursor_area)]);
        for session in [
            ImeSessionId {
                owner: 2,
                generation: 0,
            },
            ImeSessionId {
                owner: 2,
                generation: 1,
            },
        ] {
            log.clear();
            request.session = session;
            sync.sync(Some(request), |c| log.push(c)).unwrap();
            assert_eq!(
                log,
                [
                    Command::Enabled(false),
                    Command::CursorArea(request.cursor_area),
                    Command::Enabled(true)
                ]
            );
        }
        log.clear();
        sync.sync(None, |c| log.push(c)).unwrap();
        sync.sync(None, |c| log.push(c)).unwrap();
        assert_eq!(log, [Command::Enabled(false)]);
    }
    #[test]
    fn invalid_area_disables_the_previous_session_before_returning_error() {
        let mut sync = ImeSync::default();
        let mut request = ImeRequest {
            session: ImeSessionId {
                owner: 1,
                generation: 0,
            },
            cursor_area: Rect::new(0.0, 0.0, 0.0, 20.0),
        };
        sync.sync(Some(request), |_| {}).unwrap();
        request.cursor_area.origin.x = f64::NAN;
        let mut log = Vec::new();
        assert!(sync.sync(Some(request), |c| log.push(c)).is_err());
        assert_eq!(log, [Command::Enabled(false)]);
    }
}
