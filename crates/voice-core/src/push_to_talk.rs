//! Push-to-talk recording state machine.
//!
//! This module wires hotkey press/release events to audio capture start/stop.
//! ASR, clipboard, and paste are intentionally left to later Step 4 PRs.

use crate::capture::{AudioCapture, AudioFormat, CaptureError, CaptureSession, PcmSample};
use crate::hotkey::PushToTalkEvent;

/// Audio captured during one push-to-talk utterance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAudio {
    pub format: AudioFormat,
    pub samples: Vec<PcmSample>,
}

/// Events emitted by [`PushToTalkRecorder`] while handling hotkey input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushToTalkRecorderEvent {
    RecordingStarted(AudioFormat),
    RecordingStopped(RecordedAudio),
}

/// Connects push-to-talk hotkey events to an [`AudioCapture`] backend.
pub struct PushToTalkRecorder<C> {
    capture: C,
    requested_format: AudioFormat,
    active: Option<ActiveRecording>,
}

impl<C: AudioCapture> PushToTalkRecorder<C> {
    pub fn new(capture: C, requested_format: AudioFormat) -> Self {
        Self {
            capture,
            requested_format,
            active: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    /// Drain currently available PCM chunks from the active capture session.
    pub fn poll_audio(&mut self) {
        let Some(active) = &mut self.active else {
            return;
        };

        while let Ok(chunk) = active.session.rx.try_recv() {
            active.samples.extend(chunk);
        }
    }

    /// Apply one push-to-talk event to the recording state machine.
    pub fn handle_hotkey_event(
        &mut self,
        event: PushToTalkEvent,
    ) -> Result<Option<PushToTalkRecorderEvent>, CaptureError> {
        self.poll_audio();

        match event {
            PushToTalkEvent::Pressed if self.active.is_none() => {
                let session = self.capture.start(self.requested_format)?;
                let actual = session.format;
                self.active = Some(ActiveRecording {
                    session,
                    samples: Vec::new(),
                });
                Ok(Some(PushToTalkRecorderEvent::RecordingStarted(actual)))
            }
            PushToTalkEvent::Released if self.active.is_some() => {
                self.poll_audio();
                let active = self.active.take().expect("checked active recording");
                let ActiveRecording { session, samples } = active;
                let format = session.format;
                drop(session);

                Ok(Some(PushToTalkRecorderEvent::RecordingStopped(
                    RecordedAudio { format, samples },
                )))
            }
            _ => Ok(None),
        }
    }
}

struct ActiveRecording {
    session: CaptureSession,
    samples: Vec<PcmSample>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{CaptureSession, PcmChunk, StopHandle};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};

    struct MockCapture {
        format: AudioFormat,
        sessions: VecDeque<Vec<PcmChunk>>,
        starts: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
    }

    impl MockCapture {
        fn new(format: AudioFormat, sessions: Vec<Vec<PcmChunk>>) -> Self {
            Self {
                format,
                sessions: sessions.into(),
                starts: Arc::new(AtomicUsize::new(0)),
                stops: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl AudioCapture for MockCapture {
        fn start(&mut self, _: AudioFormat) -> Result<CaptureSession, CaptureError> {
            self.starts.fetch_add(1, Ordering::Relaxed);
            let chunks = self
                .sessions
                .pop_front()
                .ok_or_else(|| CaptureError::Backend("no mock session".to_string()))?;
            let (tx, rx) = mpsc::channel();
            for chunk in chunks {
                tx.send(chunk).unwrap();
            }
            drop(tx);

            Ok(CaptureSession {
                format: self.format,
                rx,
                stop: Box::new(MockStop {
                    stops: Arc::clone(&self.stops),
                }),
            })
        }
    }

    struct MockStop {
        stops: Arc<AtomicUsize>,
    }

    impl Drop for MockStop {
        fn drop(&mut self) {
            self.stops.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl StopHandle for MockStop {}

    #[test]
    fn press_starts_and_release_stops_recording() {
        let format = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let capture = MockCapture::new(format, vec![vec![vec![1, 2], vec![3]]]);
        let starts = Arc::clone(&capture.starts);
        let stops = Arc::clone(&capture.stops);
        let mut recorder = PushToTalkRecorder::new(capture, format);

        let event = recorder
            .handle_hotkey_event(PushToTalkEvent::Pressed)
            .unwrap();
        assert_eq!(
            event,
            Some(PushToTalkRecorderEvent::RecordingStarted(format))
        );
        assert!(recorder.is_recording());

        recorder.poll_audio();
        let event = recorder
            .handle_hotkey_event(PushToTalkEvent::Released)
            .unwrap();

        assert_eq!(
            event,
            Some(PushToTalkRecorderEvent::RecordingStopped(RecordedAudio {
                format,
                samples: vec![1, 2, 3],
            }))
        );
        assert!(!recorder.is_recording());
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(stops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn duplicate_press_and_idle_release_are_ignored() {
        let format = AudioFormat {
            sample_rate: 16_000,
            channels: 1,
        };
        let capture = MockCapture::new(format, vec![vec![vec![42]]]);
        let starts = Arc::clone(&capture.starts);
        let mut recorder = PushToTalkRecorder::new(capture, format);

        assert_eq!(
            recorder
                .handle_hotkey_event(PushToTalkEvent::Released)
                .unwrap(),
            None
        );
        assert!(recorder
            .handle_hotkey_event(PushToTalkEvent::Pressed)
            .unwrap()
            .is_some());
        assert_eq!(
            recorder
                .handle_hotkey_event(PushToTalkEvent::Pressed)
                .unwrap(),
            None
        );

        assert_eq!(starts.load(Ordering::Relaxed), 1);
    }
}
