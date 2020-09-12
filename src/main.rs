use std::error::Error;
use std::result::Result;
use std::time::Duration;
use std::vec::Vec;

// use tokio::time::delay_for;
use tokio::sync::{mpsc, oneshot};
use tokio::runtime;

fn main() -> Result<(), Box<dyn Error>> {
    let mut rt = runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let (tx, mut rx) = mpsc::channel(32);

        tokio::spawn(async {
            http_api::serve(tx).await;
        });

        // let mut tx2 = tx.clone();

        // tokio::spawn(async move {
        //     tick(tx).await;
        // });

        // tokio::spawn(async move {
        //     signal::ctrl_c().await.unwrap();
        //     tx2.send(String::from("ctrl-c")).await.unwrap();
        // });

        let mut timers = Vec::new();
        timers.push(Some(timer::Timer::new(0, String::from("default"), Duration::new(1 * 60, 0))));

        while let Some(msg) = rx.recv().await {
            match msg {
                msg::Message::ListTimers(tx) => list_timers(tx, &timers),
                msg::Message::GetTimer(tx, id) => get_timer(tx, &timers, id),
                _ => println!("main: unknown message"),
            }
        }
    });

    Ok(())
}

fn list_timers(tx: oneshot::Sender<msg::Message>, timers: &Vec<Option<timer::Timer>>) {
    let mut vec = Vec::new();

    for timer in timers {
        match timer {
            Some(t) => vec.push(t.clone()),
            None => (),
        }
    }

    if let Err(_) = tx.send(msg::Message::Timers(vec)) {
        println!("error sending Timers message");
    }
}

fn get_timer(tx: oneshot::Sender<msg::Message>, timers: &Vec<Option<timer::Timer>>, id: u16) {
    let message: msg::Message;

    if let Some(index) = timers.get(usize::from(id)) {
        match index {
            Some(timer) => message = msg::Message::Timer(timer.clone()),
            None => message = msg::Message::Error(format!("timer {} does not exist", id)),
        }
    } else {
        message = msg::Message::Error(format!("timer {} does not exist", id));
    }

    if let Err(_) = tx.send(message) {
        println!("error sending Timer message");
    }
}

//  _   _
// | |_(_)_ __ ___   ___ _ __
// | __| | '_ ` _ \ / _ \ '__|
// | |_| | | | | | |  __/ |
//  \__|_|_| |_| |_|\___|_|
//

mod timer {
    use std::error::Error;
    use std::fmt;
    use std::time::{Duration, Instant, SystemTime};

    use serde::Serialize;

    #[derive(Debug)]
    pub struct TimerError {
        details: String,
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    pub enum TimerState {
        Initial,
        Running,
        Paused,
        Completed,
    }

    #[derive(Clone, Serialize)]
    pub struct Timer {
        pub id: u16,
        pub name: String,
        pub state: TimerState,
        pub duration: Duration,
        pub elapsed: Duration,
        pub interruptions: u16,
        pub start_time: SystemTime,
        pub end_time: SystemTime,

        #[serde(skip_serializing)]
        intervals: u64,
        #[serde(skip_serializing)]
        interval_start: Instant,
    }

    impl Timer {
        pub fn new(id: u16, name: String, duration: Duration) -> Timer {
            Timer {
                id: id,
                name: name,
                state: TimerState::Initial,
                duration: duration,
                elapsed: Duration::new(0, 0),
                interruptions: 0,
                start_time: SystemTime::UNIX_EPOCH,
                end_time: SystemTime::UNIX_EPOCH,
                intervals: 0,
                interval_start: Instant::now(), // FIXME
            }
        }

        pub fn set_state(&mut self, state: TimerState) -> Result<(), TimerError> {
            match self.state {
                TimerState::Initial => match state {
                    TimerState::Running => return self.start(),
                    _ => return Err(TimerError::new("set_state: invalid state transition")),
                },

                TimerState::Running => match state {
                    TimerState::Paused => return self.pause(),
                    _ => return Err(TimerError::new("set_state: invalid state transition")),
                },

                TimerState::Paused => match state {
                    TimerState::Running => self.resume(),
                    _ => return Err(TimerError::new("set_state: invalid state transition")),
                },

                TimerState::Completed => Err(TimerError::new("set_state: invalid state transition")),
            }
        }

        fn start(&mut self) -> Result<(), TimerError> {
            if self.state != TimerState::Initial {
                return Err(TimerError::new("start: timer must be in 'Initial' state"));
            }
            self.state = TimerState::Running;
            self.start_time = SystemTime::now();
            self.interval_start = Instant::now();
            Ok(())
        }

        fn pause(&mut self) -> Result<(), TimerError> {
            if self.state != TimerState::Running {
                return Err(TimerError::new("pause: timer must be in 'Running' state"));
            }

            // call tick to update elapsed
            self.tick();

            // if the tick pushed elapsed >= duration, the timer has finished
            // and there's no need to pause
            if self.state == TimerState::Completed {
                return Ok(());
            }

            self.state = TimerState::Paused;
            self.interruptions += 1;
            Ok(())
        }

        fn resume(&mut self) -> Result<(), TimerError> {
            if self.state != TimerState::Paused {
                return Err(TimerError::new("resume: timer must be in 'Paused' state"));
            }

            self.state = TimerState::Running;
            self.interval_start = Instant::now();
            Ok(())
        }

        pub fn tick(&mut self) {
            if self.state != TimerState::Running {
                return;
            }

            let now = Instant::now();
            let interval_length = now.duration_since(self.interval_start);
            self.elapsed += interval_length;
            self.intervals += 1;
            self.interval_start = now;

            if self.elapsed >= self.duration {
                // just clean it up if it went over a bit
                self.elapsed = self.duration;
                self.state = TimerState::Completed;
                self.end_time = SystemTime::now();
            }
        }
    }

    impl TimerError {
        fn new(msg: &str) -> TimerError {
            TimerError {
                details: msg.to_string(),
            }
        }
    }

    impl fmt::Display for TimerError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.details)
        }
    }

    impl Error for TimerError {
        fn description(&self) -> &str {
            &self.details
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::thread::sleep;

        #[test]
        fn timer_start_initial_state() {
            let mut t = Timer::new(0, String::from("default"), Duration::new(60, 0));
            let init_interval = t.interval_start;
            assert_eq!(t.state, TimerState::Initial);
            assert_eq!(t.start_time, SystemTime::UNIX_EPOCH);
            t.start().expect("failed starting timer");
            assert_eq!(t.state, TimerState::Running);
            assert_ne!(t.interval_start, init_interval);
            assert_ne!(t.start_time, SystemTime::UNIX_EPOCH);
        }

        #[test]
        fn timer_start_fail_other_states() {
            let mut t = Timer::new(0, String::from("default"), Duration::new(60, 0));

            t.state = TimerState::Running;
            match t.start() {
                Ok(()) => panic!("start function should error on 'Running' state"),
                _ => (),
            }

            t.state = TimerState::Paused;
            match t.start() {
                Ok(()) => panic!("start function should error on 'Paused' state"),
                _ => (),
            }

            t.state = TimerState::Completed;
            match t.start() {
                Ok(()) => panic!("start function should error on 'Completed' state"),
                _ => (),
            }
        }

        #[test]
        fn timer_start_and_stop() {
            let mut t = Timer::new(0, String::from("default"), Duration::new(60, 0));
            t.set_state(TimerState::Running).unwrap();
            sleep(Duration::new(1, 0));
            t.set_state(TimerState::Paused).unwrap();

            assert_eq!(t.elapsed.as_secs(), 1);
            assert_eq!(t.interruptions, 1);
            assert_eq!(t.intervals, 1);
            assert_eq!(t.state, TimerState::Paused);
        }

        #[test]
        fn timer_complete() {
            let mut t = Timer::new(0, String::from("default"), Duration::new(0, 500_000_000));
            t.set_state(TimerState::Running).unwrap();
            sleep(Duration::new(1, 0));
            t.tick();

            assert_eq!(t.interruptions, 0);
            assert_eq!(t.intervals, 1);
            assert_ne!(t.end_time, SystemTime::UNIX_EPOCH);
            assert_eq!(t.state, TimerState::Completed);
        }
    }
}


//  _     _   _                       _
// | |__ | |_| |_ _ __     __ _ _ __ (_)
// | '_ \| __| __| '_ \   / _` | '_ \| |
// | | | | |_| |_| |_) | | (_| | |_) | |
// |_| |_|\__|\__| .__/   \__,_| .__/|_|
//               |_|           |_|

mod http_api {
    use tokio::sync::mpsc::Sender;
    use super::msg::Message;

    pub async fn serve(chan: Sender<Message>) {
        let api = filters::tomayto(chan);
        warp::serve(api)
            .run(([127, 0, 0, 1], 3030))
            .await;
    }

    mod filters {
        use tokio::sync::mpsc::Sender;
        use warp::Filter;
        use super::handlers;
        use super::super::msg::Message;

        pub fn tomayto(chan: Sender<Message>) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
            timers(chan.clone())
                .or(timer(chan.clone()))
        }

        fn timers(chan: Sender<Message>) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
            warp::path!("timers")
                .and(warp::get())
                .and(warp::any().map(move || chan.clone()))
                .and_then(handlers::list_timers)
        }

        fn timer(chan: Sender<Message>) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
            warp::path!("timers" / u16)
                .and(warp::get())
                .and(warp::any().map(move || chan.clone()))
                .and_then(handlers::get_timer)
        }
    }

    mod handlers {
        use std::convert::Infallible;

        use serde::Serialize;
        use tokio::sync::mpsc::Sender;
        use tokio::sync::oneshot;
        use tokio::time::{Duration, timeout};
        use warp::http::StatusCode;
        use warp::reply;

        use super::super::msg::Message;

        #[derive(Serialize)]
        struct Error {
            pub error: String,
        }

        impl Error {
            fn json(err: String) -> warp::reply::Json {
                reply::json(&Error{error: err})
            }
        }

        pub async fn list_timers(mut chan: Sender<Message>) -> Result<impl warp::Reply, Infallible> {
            const TIMEOUT_MS:u64 = 250;

            let (tx, rx) = oneshot::channel::<Message>();

            if let Err(e) = chan.send(Message::ListTimers(tx)).await {
                let json = Error::json(format!("failed to send message to main task: {}", e));
                return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
            }

            match timeout(Duration::from_millis(TIMEOUT_MS), rx).await {
                // got a message
                Ok(msg) => {
                    match msg {
                        Ok(Message::Timers(timers)) => {
                            let json = reply::json(&timers);
                            return Ok(reply::with_status(json, StatusCode::OK));
                        }
                        Ok(Message::Error(err)) => {
                            let json = Error::json(err);
                            return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                        }
                        _ => {
                            let json = Error::json(format!("received unexpected message type from main task"));
                            return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                        }
                    }
                }
                // timed out waiting for message
                Err(_) => {
                    let json = Error::json(format!("did not receive message from main task withn {}ms timeout", TIMEOUT_MS));
                    return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                }
            }
        }

        pub async fn get_timer(id: u16, mut chan: Sender<Message>) -> Result<impl warp::Reply, Infallible> {
            const TIMEOUT_MS:u64 = 250;

            let (tx, rx) = oneshot::channel::<Message>();

            if let Err(e) = chan.send(Message::GetTimer(tx, id)).await {
                let json = Error::json(format!("failed to send message to main task: {}", e));
                return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
            }

            match timeout(Duration::from_millis(TIMEOUT_MS), rx).await {
                // got a message
                Ok(msg) => {
                    match msg {
                        Ok(Message::Timer(timer)) => {
                            let json = reply::json(&timer);
                            return Ok(reply::with_status(json, StatusCode::OK));
                        }
                        Ok(Message::Error(err)) => {
                            let json = Error::json(err);
                            return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                        }
                        _ => {
                            let json = Error::json(format!("received unexpected message type from main task"));
                            return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                        }
                    }
                }
                // timed out waiting for message
                Err(_) => {
                    let json = Error::json(format!("did not receive message from main task withn {}ms timeout", TIMEOUT_MS));
                    return Ok(reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR));
                }
            }
        }
    }
}

mod msg {
    use std::vec::Vec;
    use tokio::sync::oneshot::Sender;

    pub enum Message {
        // requests
        ListTimers(Sender<Message>),
        GetTimer(Sender<Message>, u16),
        // DeleteTimer(Sender<Message>, u16),
        // CreateTimer{ tx: Sender<Message>, id: u16, duration_secs: u32},
        // SetTimerState(Sender<Message>, u16, super::timer::TimerState),

        // responses
        Error(String),
        Timers(Vec<super::timer::Timer>),
        Timer(super::timer::Timer),
    }
}
