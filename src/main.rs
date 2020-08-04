use serenity::{model::prelude::*, prelude::*, utils::Color};
use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::sync::Arc;

pub type JoeResult<T> = Result<T, Box<dyn Error>>;

pub const EMBED_COLOR: Color = Color::new(0x7a4c50);

mod commands;
mod messages;
mod storage;
mod utils;

struct MessageHandlers {
    taki: commands::Taki,
    chain: commands::Chain,
    poll: commands::Poll,
    joker: commands::Joker,
    wdyt: commands::Wdyt,
    img2msg: commands::Img2msg,
}

struct Handler {
    bot_user: Mutex<RefCell<Option<CurrentUser>>>,
    bot_channel_id: ChannelId,
    message_handlers: Mutex<MessageHandlers>,
}

impl Handler {
    fn handle_message(&self, ctx: Context, msg: Message) -> JoeResult<()> {
        if let Some(ref user) = *self.bot_user.lock().borrow() {
            if user.id == msg.author.id {
                return Ok(());
            }
        }

        let mut handlers = self.message_handlers.lock();
        let taki_result = handlers.taki.handle_message(&ctx, &msg);
        if taki_result.map_err(|e| format!("Taki: {:?}", e))? {
            return Ok(());
        }
        let chain_result = handlers.chain.handle_message(&ctx, &msg);
        if chain_result.map_err(|e| format!("Chain: {:?}", e))? {
            return Ok(());
        }
        let poll_result = handlers.poll.handle_message(&ctx, &msg);
        if poll_result.map_err(|e| format!("Poll: {:?}", e))? {
            return Ok(());
        }
        let joker_result = handlers.joker.handle_message(&ctx, &msg);
        if joker_result.map_err(|e| format!("Joker: {:?}", e))? {
            return Ok(());
        }
        let wdyt_result = handlers.wdyt.handle_message(&ctx, &msg);
        if wdyt_result.map_err(|e| format!("Wdyt: {:?}", e))? {
            return Ok(());
        }
        let img2msg_result = handlers.img2msg.handle_message(&ctx, &msg);
        if img2msg_result.map_err(|e| format!("Img2msg: {:?}", e))? {
            return Ok(());
        }
        if msg.content.starts_with('!') {
            msg.channel_id
                .send_message(&ctx.http, |m| {
                    m.embed(|e| {
                        e.color(EMBED_COLOR);
                        e.title("Joe's Saloon");
                        e.field(
                            "таки",
                            r#"
`!takistart` — начнем партию
`!takisuspects` — бросим взгляд на плакаты о розыске
`!takistats` — поднимем бокал крепкого виски за самых метких стрелков
                    "#,
                            false,
                        );
                        e.field(
                            "мэшап",
                            r#"
`!mashup` — узнаем от бармена последние слухи
`!mashupmore` — посплетничаем еще
`!mashupstars` — поприветствуем жителей городка
"#,
                            false,
                        );
                        e.field(
                            "политика",
                            r#"
`!poll` — устроим честный суд
"#,
                            false,
                        );
                        e.field(
                            "поговорим с джо",
                            r#"
`что думаешь об итмо и бонче`
`джокер++`
"#,
                            false,
                        );
                        e.field(
                            "займемся делом",
                            "_покажи джо фотокарточку, о которой хочешь разузнать побольше_",
                            false,
                        );
                        e
                    });
                    m
                })
                .map_err(|e| format!("Help: {:?}", e))?;
        }
        Ok(())
    }
}

impl EventHandler for Handler {
    fn ready(&self, _: Context, ready: Ready) {
        println!("Connected as {}", ready.user.name);
        self.bot_user.lock().replace(Some(ready.user));
    }

    fn message(&self, ctx: Context, msg: Message) {
        if msg.channel_id != self.bot_channel_id {
            return;
        }
        if let Err(e) = self.handle_message(ctx, msg) {
            eprintln!("{}", e)
        }
    }
}

fn main() {
    let bot_token = std::env::var("BOT_TOKEN")
        .expect("Provide a valid bot token via the BOT_TOKEN environment variable");
    let bot_channel_id: u64 = std::env::var("BOT_CHANNEL_ID")
        .ok()
        .and_then(|id| id.parse().ok())
        .expect("Provide the bot's channel id via the BOT_CHANNEL_ID environment variable");
    let redis = storage::Redis::new("redis://127.0.0.1/")
        .map_err(|e| format!("redis: {}", e))
        .unwrap();

    let message_handlers = Mutex::new(init_handlers(bot_channel_id, &redis));
    let handler = Handler {
        bot_user: Mutex::new(RefCell::new(None)),
        bot_channel_id: ChannelId(bot_channel_id),
        message_handlers,
    };
    let mut client = Client::new(&bot_token, handler).unwrap();

    if let Err(e) = client.start() {
        eprintln!("Client error: {:?}", e);
    }
}

fn init_handlers(channel_id: u64, redis: &storage::Redis) -> MessageHandlers {
    let messages = init_messages();
    let chain_data: joebot_markov_chain::MarkovChain =
        bincode::deserialize_from(File::open("chain.bin").unwrap()).unwrap();

    let taki = commands::Taki::new(messages.clone(), channel_id, redis);
    let chain = commands::Chain::new(chain_data);
    let poll = commands::Poll::new();
    let joker = commands::Joker::new(messages.clone()).unwrap();
    let wdyt = commands::Wdyt::new(messages.clone()).unwrap();
    let img2msg = commands::Img2msg::new(messages.clone()).unwrap();

    MessageHandlers {
        taki,
        chain,
        poll,
        joker,
        wdyt,
        img2msg,
    }
}

fn init_messages() -> Arc<messages::MessageDump> {
    let msg_name_env = std::env::var("MSG_NAMES").unwrap_or_default();
    let msg_names: HashSet<&str> = msg_name_env.split(',').map(|n| n.trim()).collect();

    let messages = messages::MessageDump::from_file("messages.html", &msg_names);
    let message_authors = messages
        .authors
        .iter()
        .map(|a| a.full_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "{} messages from the following authors: {}\n",
        messages.texts.len(),
        message_authors
    );
    Arc::new(messages)
}
