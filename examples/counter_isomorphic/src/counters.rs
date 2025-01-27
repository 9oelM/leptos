use cfg_if::cfg_if;
use leptos::*;
use leptos_meta::*;
use leptos_router::*;

cfg_if! {
    if #[cfg(feature = "ssr")] {
        use std::sync::atomic::{AtomicI32, Ordering};
        use broadcaster::BroadcastChannel;
        static COUNT: AtomicI32 = AtomicI32::new(0);

        lazy_static::lazy_static! {
            pub static ref COUNT_CHANNEL: BroadcastChannel<i32> = BroadcastChannel::new();
        }
    }
}

// "/api" is an optional prefix that allows you to locate server functions wherever you'd like on the server
#[server(GetServerCount, "/api")]
pub async fn get_server_count() -> Result<i32, ServerFnError> {
    Ok(COUNT.load(Ordering::Relaxed))
}

#[server(AdjustServerCount, "/api")]
pub async fn adjust_server_count(
    delta: i32,
    msg: String,
) -> Result<i32, ServerFnError> {
    let new = COUNT.load(Ordering::Relaxed) + delta;
    COUNT.store(new, Ordering::Relaxed);
    _ = COUNT_CHANNEL.send(&new).await;
    println!("message = {:?}", msg);
    Ok(new)
}

#[server(ClearServerCount, "/api")]
pub async fn clear_server_count() -> Result<i32, ServerFnError> {
    COUNT.store(0, Ordering::Relaxed);
    _ = COUNT_CHANNEL.send(&0).await;
    Ok(0)
}
#[component]
pub fn Counters(cx: Scope) -> impl IntoView {
    provide_meta_context(cx);
    view! { cx,
        <Router>
            <header>
                <h1>"Server-Side Counters"</h1>
                <p>"Each of these counters stores its data in the same variable on the server."</p>
                <p>
                    "The value is shared across connections. Try opening this is another browser tab to see what I mean."
                </p>
            </header>
            <nav>
                <ul>
                    <li>
                        <A href="">"Simple"</A>
                    </li>
                    <li>
                        <A href="form">"Form-Based"</A>
                    </li>
                    <li>
                        <A href="multi">"Multi-User"</A>
                    </li>
                </ul>
            </nav>
            <Link rel="shortcut icon" type_="image/ico" href="/favicon.ico"/>
            <main>
                <Routes>
                    <Route path="" view=Counter/>
                    <Route path="form" view=FormCounter/>
                    <Route path="multi" view=MultiuserCounter/>
                    <Route path="multi" view=NotFound/>
                </Routes>
            </main>
        </Router>
    }
}

// This is an example of "single-user" server functions
// The counter value is loaded from the server, and re-fetches whenever
// it's invalidated by one of the user's own actions
// This is the typical pattern for a CRUD app
#[component]
pub fn Counter(cx: Scope) -> impl IntoView {
    let dec = create_action(cx, |_| adjust_server_count(-1, "decing".into()));
    let inc = create_action(cx, |_| adjust_server_count(1, "incing".into()));
    let clear = create_action(cx, |_| clear_server_count());
    let counter = create_resource(
        cx,
        move || {
            (
                dec.version().get(),
                inc.version().get(),
                clear.version().get(),
            )
        },
        |_| get_server_count(),
    );

    let value = move || {
        counter
            .read(cx)
            .map(|count| count.unwrap_or(0))
            .unwrap_or(0)
    };
    let error_msg = move || {
        counter.read(cx).and_then(|res| match res {
            Ok(_) => None,
            Err(e) => Some(e),
        })
    };

    view! { cx,
        <div>
            <h2>"Simple Counter"</h2>
            <p>
                "This counter sets the value on the server and automatically reloads the new value."
            </p>
            <div>
                <button on:click=move |_| clear.dispatch(())>"Clear"</button>
                <button on:click=move |_| dec.dispatch(())>"-1"</button>
                <span>"Value: " {value} "!"</span>
                <button on:click=move |_| inc.dispatch(())>"+1"</button>
            </div>
            {move || {
                error_msg()
                    .map(|msg| {
                        view! { cx, <p>"Error: " {msg.to_string()}</p> }
                    })
            }}
        </div>
    }
}

// This is the <Form/> counter
// It uses the same invalidation pattern as the plain counter,
// but uses HTML forms to submit the actions
#[component]
pub fn FormCounter(cx: Scope) -> impl IntoView {
    let adjust = create_server_action::<AdjustServerCount>(cx);
    let clear = create_server_action::<ClearServerCount>(cx);

    let counter = create_resource(
        cx,
        move || (adjust.version().get(), clear.version().get()),
        |_| {
            log::debug!("FormCounter running fetcher");
            get_server_count()
        },
    );
    let value = move || {
        log::debug!("FormCounter looking for value");
        counter.read(cx).and_then(|n| n.ok()).unwrap_or(0)
    };

    view! { cx,
        <div>
            <h2>"Form Counter"</h2>
            <p>
                "This counter uses forms to set the value on the server. When progressively enhanced, it should behave identically to the “Simple Counter.”"
            </p>
            <div>
                <ActionForm action=clear>
                    <input type="submit" value="Clear"/>
                </ActionForm>
                <ActionForm action=adjust>
                    <input type="hidden" name="delta" value="-1"/>
                    <input type="hidden" name="msg" value="form value down"/>
                    <input type="submit" value="-1"/>
                </ActionForm>
                <span>"Value: " {move || value().to_string()} "!"</span>
                <ActionForm action=adjust>
                    <input type="hidden" name="delta" value="1"/>
                    <input type="hidden" name="msg" value="form value up"/>
                    <input type="submit" value="+1"/>
                </ActionForm>
            </div>
        </div>
    }
}

// This is a kind of "multi-user" counter
// It relies on a stream of server-sent events (SSE) for the counter's value
// Whenever another user updates the value, it will update here
// This is the primitive pattern for live chat, collaborative editing, etc.
#[component]
pub fn MultiuserCounter(cx: Scope) -> impl IntoView {
    let dec =
        create_action(cx, |_| adjust_server_count(-1, "dec dec goose".into()));
    let inc =
        create_action(cx, |_| adjust_server_count(1, "inc inc moose".into()));
    let clear = create_action(cx, |_| clear_server_count());

    #[cfg(not(feature = "ssr"))]
    let multiplayer_value = {
        use futures::StreamExt;

        let mut source =
            gloo_net::eventsource::futures::EventSource::new("/api/events")
                .expect("couldn't connect to SSE stream");
        let s = create_signal_from_stream(
            cx,
            source
                .subscribe("message")
                .unwrap()
                .map(|value| match value {
                    Ok(value) => value
                        .1
                        .data()
                        .as_string()
                        .expect("expected string value"),
                    Err(_) => "0".to_string(),
                }),
        );

        on_cleanup(cx, move || source.close());
        s
    };

    #[cfg(feature = "ssr")]
    let (multiplayer_value, _) = create_signal(cx, None::<i32>);

    view! { cx,
        <div>
            <h2>"Multi-User Counter"</h2>
            <p>
                "This one uses server-sent events (SSE) to live-update when other users make changes."
            </p>
            <div>
                <button on:click=move |_| clear.dispatch(())>"Clear"</button>
                <button on:click=move |_| dec.dispatch(())>"-1"</button>
                <span>
                    "Multiplayer Value: " {move || multiplayer_value.get().unwrap_or_default()}
                </span>
                <button on:click=move |_| inc.dispatch(())>"+1"</button>
            </div>
        </div>
    }
}

#[component]
fn NotFound(cx: Scope) -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        let resp = expect_context::<leptos_actix::ResponseOptions>(cx);
        resp.set_status(actix_web::http::StatusCode::NOT_FOUND);
    }

    view! { cx, <h1>"Not Found"</h1> }
}
