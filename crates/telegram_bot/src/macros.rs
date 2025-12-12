//! Utilities macro for sending and tracing requests
#[macro_export]
macro_rules! request {
    ($verb:ident, $client:expr, $url:expr, $user_id:expr,$json:expr) => {
        $client
            .$verb($url)
            .header("telegram-user-id", $user_id)
            .json($json)
            .send()
            .await
            .unwrap()
    };
}

#[macro_export]
macro_rules! post {
    ($client:expr, $url:expr, $user_id:expr,$json:expr) => {
        $crate::request!(post, $client, $url, $user_id, $json)
    };
}

#[macro_export]
macro_rules! get {
    ($client:expr, $url:expr, $user_id:expr,$json:expr) => {
        $crate::request!(post, $client, $url, $user_id, $json)
    };
}

#[macro_export]
macro_rules! delete {
    ($client:expr, $url:expr, $user_id:expr,$json:expr) => {
        $crate::request!(post, $client, $url, $user_id, $json)
    };
}

#[macro_export]
macro_rules! request_check {
    ($verb:ident, $client:expr, $url:expr, $user_id:expr,$json:expr, $success_cond:pat, $success:expr, $failure:expr) => {{
        let response = $crate::request!($verb, $client, $url, $user_id, $json);

        match response.status() {
            $success_cond => ($success, Some(response)),
            _ => {
                tracing::debug!("{:?}", response);
                tracing::debug!("body: {}", response.text().await.unwrap());

                ($failure, None)
            }
        }
    }};

    ($verb:ident, $client:expr, $url:expr, $user_id:expr,$json:expr, $success:expr, $failure:expr) => {{
        let response = $crate::request!($verb, $client, $url, $user_id, $json);

        match response.status() {
            StatusCode::OK => ($success, Some(response)),
            _ => {
                tracing::debug!("{:?}", response);
                tracing::debug!("body: {}", response.text().await.unwrap());

                ($failure, None)
            }
        }
    }};
}

#[macro_export]
macro_rules! post_check {
    ($client:expr, $url:expr, $user_id:expr,$json:expr, $success_cond:pat, $success:expr, $failure:expr) => {
        $crate::request_check!(
            post,
            $client,
            $url,
            $user_id,
            $json,
            $success_cond,
            $success,
            $failure
        )
    };

    ($client:expr, $url:expr, $user_id:expr,$json:expr, $success:expr, $failure:expr) => {
        $crate::request_check!(post, $client, $url, $user_id, $json, $success, $failure)
    };
}

#[macro_export]
macro_rules! get_check {
    ($client:expr, $url:expr, $user_id:expr, $json:expr, $success_cond:pat, $success:expr, $failure:expr) => {
        $crate::request_check!(
            get,
            $client,
            $url,
            $user_id,
            $json,
            $success_cond,
            $success,
            $failure
        )
    };

    ($client:expr, $url:expr, $user_id:expr, $json:expr, $success:expr, $failure:expr) => {
        $crate::request_check!(get, $client, $url, $user_id, $json, $success, $failure)
    };
}

#[macro_export]
macro_rules! delete_check {
    ($client:expr, $url:expr, $user_id:expr, $json:expr, $success_cond:pat, $success:expr, $failure:expr) => {
        $crate::request_check!(
            delete,
            $client,
            $url,
            $user_id,
            $json,
            $success_cond,
            $success,
            $failure
        )
    };

    ($client:expr, $url:expr, $user_id:expr, $json:expr, $success:expr, $failure:expr) => {
        $crate::request_check!(delete, $client, $url, $user_id, $json, $success, $failure)
    };
}
