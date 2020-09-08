use std::error::Error;
use std::result::Result;
use std::time::Duration;

// use tokio::time::delay_for;
use tokio::sync::mpsc;
use tokio::runtime;
// use tokio::signal;

// #[tokio::main]
// pub async fn main() -> Result<(), ()> {
//     println!("tomaytod");
//     // let t = Timer::new(Duration::new(60, 0));
//     // println!("elapsed {}", t.elapsed_secs());
//     tokio::spawn(async move {
//         tick().await;
//     });

//     loop {
//         delay_for(Duration::from_millis(1_000)).await;
//     }
//     Ok(())
// }

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

        let mut timer = timer::Timer::new(Duration::new(25 * 60, 0));
        timer.set_state(timer::TimerState::Running).unwrap();

        while let Some(msg) = rx.recv().await {
            // println!("message = {}", msg);
            timer.tick();
            println!("timer elapsed = {}", timer.elapsed.as_secs());
            match msg {
                msg::Message::ListTimers(tx) => {
                    let mut vec = std::vec::Vec::new();
                    vec.push(timer.clone());
                    tx.send(msg::Message::Timers(vec));
                }

                msg::Message::GetTimer(tx, id) => {
                    // if id < 0 || id >= timers.len() {
                    tx.send(msg::Message::Timer(timer.clone()));
                }
                _ => println!("main: Unknown"),
            }
            // match msg {
            //     messages::TomaytoMessage::TimerStatusRequest(id) => println!("TimerStatusRequest"),
            //     _ => (),
            // }
        }
    });

    Ok(())
}

// async fn tick(mut tx: mpsc::Sender<String>) {
//     let mut t = Timer::new(Duration::new(60, 0));
//     loop {
//         delay_for(Duration::from_millis(1_000)).await;
//         t.tick();
//         tx.send(String::from("tick")).await.unwrap();
//     }
// }

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

    #[derive(Debug)]
    pub struct TimerError {
        details: String,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum TimerState {
        Initial,
        Running,
        Paused,
        Completed,
    }

    #[derive(Clone)]
    pub struct Timer {
        pub state: TimerState,
        pub duration: Duration,
        pub elapsed: Duration,
        pub interruptions: u16,
        pub start_time: SystemTime,
        pub end_time: SystemTime,

        intervals: u64,
        interval_start: Instant,
    }

    impl Timer {
        pub fn new(duration: Duration) -> Timer {
            Timer {
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
            let mut t = Timer::new(Duration::new(60, 0));
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
            let mut t = Timer::new(Duration::new(60, 0));

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
            let mut t = Timer::new(Duration::new(60, 0));
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
            let mut t = Timer::new(Duration::new(0, 500_000_000));
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
        use tokio::sync::mpsc::Sender;
        use tokio::sync::oneshot;
        //use tokio::time::timeout;
        //use warp::http::StatusCode;
        use super::super::msg::Message;

        pub async fn list_timers(mut chan: Sender<Message>) -> Result<impl warp::Reply, Infallible> {
            let (tx, rx) = oneshot::channel::<Message>();

            chan.send(Message::ListTimers(tx)).await;
            // if let Err(_) = timeout(Duration::from_millis(250), rx).await {
            //     println!("ListTimers didn't hear back within 250ms");
            // }
            let mut elapsed = 0;
            match rx.await {
                Ok(msg) => {
                    match msg {
                        Message::Timers(timers) => elapsed = timers[0].elapsed.as_secs(),
                        _ => (),
                    }
                }
                _ => (),
            }

            Ok(format!("elapsed = {}", elapsed))
        }

        pub async fn get_timer(id: u16, mut chan: Sender<Message>) -> Result<impl warp::Reply, Infallible> {
            let (tx, rx) = oneshot::channel::<Message>();

            chan.send(Message::GetTimer(tx, id)).await;
            // if let Err(_) = timeout(Duration::from_millis(250), rx).await {
            //     println!("ListTimers didn't hear back within 250ms");
            // }
            let mut elapsed = 0;
            match rx.await {
                Ok(msg) => {
                    match msg {
                        Message::Timer(timer) => elapsed = timer.elapsed.as_secs(),
                        _ => (),
                    }
                }
                _ => (),
            }

            Ok(format!("elapsed = {}", elapsed))
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
