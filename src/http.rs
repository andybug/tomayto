use tokio::sync::mpsc::Sender;
use warp::Filter;

pub async fn serve_api(chan: Sender<String>) {
    let api = filters::tomayto(chan);
    warp::serve(api)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

mod filters {
    use tokio::sync::mpsc::Sender;
    use warp::Filter;
    use super::handlers;

    pub fn tomayto(chan: Sender<String>) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        timers(chan.clone())
    }

    fn timers(chan: Sender<String>) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        warp::path!("timers")
            .and(warp::get())
            .and(warp::any().map(move || chan.clone()))
            .and_then(handlers::list_timers)
    }
}

mod handlers {
    use tokio::sync::mpsc::Sender;
    //use warp::http::StatusCode;
    use std::convert::Infallible;

    pub async fn list_timers(mut chan: Sender<String>) -> Result<impl warp::Reply, Infallible> {
        // chan.send(messages::TomaytoMessage::TimerStatusRequest(0)).await.unwrap();
        chan.send(String::from("GET /timers")).await.unwrap();
        Ok("nope")
    }
}
