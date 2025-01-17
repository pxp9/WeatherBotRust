use crate::db::BotDbError;
use crate::db::Chat;
use crate::db::ClientState;
use crate::db::Forecast;
use crate::db::Repo;
use crate::deliver::ScheduleWeatherTask;
use crate::open_weather_map::client::WeatherApiClient;
use crate::open_weather_map::City;
use crate::telegram::client::ApiClient;
use crate::BotError;
use fang::async_trait;
use fang::asynk::async_queue::AsyncQueueable;
use fang::serde::Deserialize;
use fang::serde::Serialize;
use fang::typetag;
use fang::AsyncRunnable;
use fang::FangError;
use frankenstein::Update;
use frankenstein::UpdateContent;
use std::fmt::Write;
use std::str::FromStr;
use typed_builder::TypedBuilder;

const BOT_NAME: &str = "@RustWeather77Bot";
pub const TASK_TYPE: &str = "process_update";

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "fang::serde")]
pub struct ProcessUpdateTask {
    update: Update,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Command {
    Default,
    FindCity,
    SetDefaultCity,
    Start,
    Cancel,
    Schedule,
    CurrentDefaultCity,
    CurrentOffset,
    UnSchedule,
    SetOffset,
    UnknownCommand(String),
}

#[derive(TypedBuilder)]
pub struct UpdateProcessor {
    api: &'static ApiClient,
    repo: &'static Repo,
    text: String,
    message_id: i32,
    username: String,
    command: Command,
    chat: Chat,
}

impl FromStr for Command {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let command_str = s.replace(BOT_NAME, "");

        let result = match command_str.trim() {
            "/start" => Command::Start,
            "/find_city" => Command::FindCity,
            "/default" => Command::Default,
            "/set_default_city" => Command::SetDefaultCity,
            "/cancel" => Command::Cancel,
            "/schedule" => Command::Schedule,
            "/unschedule" => Command::UnSchedule,
            "/set_offset" => Command::SetOffset,
            "/current_default_city" => Command::CurrentDefaultCity,
            "/current_offset" => Command::CurrentOffset,
            _ => Command::UnknownCommand(command_str.to_string()),
        };

        Ok(result)
    }
}

impl UpdateProcessor {
    pub async fn create(update: Update) -> Result<Self, BotError> {
        if let UpdateContent::Message(message) = &update.content {
            if message.text.is_none() {
                log::error!("Update doesn't contain any text {:?}", message);

                return Err(BotError::UpdateNotMessage("no text".to_string()));
            }

            let text = message.text.clone().unwrap();

            let repo = Repo::repo().await?;
            let api = ApiClient::api_client().await;

            let chat_id: i64 = message.chat.id;
            let user = message.from.clone().expect("User not set");
            let chat = repo.find_or_create_chat(&chat_id, user.id).await?;
            let username = match user.username {
                Some(name) => format!("@{}", name),
                None => user.first_name,
            };

            let command = Command::from_str(&text).unwrap();

            let processor = Self::builder()
                .repo(repo)
                .api(api)
                .message_id(message.message_id)
                .text(text)
                .username(username)
                .chat(chat)
                .command(command)
                .build();

            Ok(processor)
        } else {
            log::error!("Update is not a message {:?}", update);

            Err(BotError::UpdateNotMessage("no message".to_string()))
        }
    }

    pub async fn process(&self) -> Result<Option<Vec<Forecast>>, BotError> {
        if self.chat.state == ClientState::Initial
            && matches!(self.command, Command::UnknownCommand(_))
        {
            return Ok(None);
        }

        self.send_typing().await?;

        if Command::Cancel == self.command {
            self.cancel(None).await?;
            return Ok(None);
        }

        match self.chat.state {
            ClientState::Initial => self.process_initial().await,

            ClientState::FindCity => {
                self.process_find_city().await?;
                Ok(None)
            }

            ClientState::SetCity => {
                self.process_set_city().await?;
                Ok(None)
            }

            ClientState::Time => {
                self.process_time().await?;
                Ok(None)
            }

            ClientState::FindCityNumber => {
                self.process_find_city_number().await?;
                Ok(None)
            }

            ClientState::SetCityNumber => {
                self.process_set_city_number().await?;
                Ok(None)
            }

            ClientState::Offset => {
                self.process_offset().await?;
                Ok(None)
            }

            ClientState::ScheduleCity => {
                self.process_schedule_city().await?;
                Ok(None)
            }

            ClientState::ScheduleCityNumber => {
                self.process_schedule_city_number().await?;
                Ok(None)
            }
        }
    }

    async fn process_initial(&self) -> Result<Option<Vec<Forecast>>, BotError> {
        match self.command {
            Command::FindCity => {
                self.repo
                    .modify_state(&self.chat.id, self.chat.user_id, ClientState::FindCity)
                    .await?;

                self.find_city_message().await?;

                Ok(None)
            }
            Command::Start => {
                self.start_message().await?;
                Ok(None)
            }
            Command::CurrentDefaultCity => {
                let text = match self.chat.default_city_id {
                    Some(id) => match self.repo.search_city_by_id(&id).await {
                        Ok(city) => format!("Your default city is {}", city),
                        Err(_) => "You do not have default city".to_string(),
                    },
                    None => "You do not have default city".to_string(),
                };
                self.send_message(&text).await?;

                Ok(None)
            }
            Command::SetDefaultCity => {
                self.set_city().await?;
                Ok(None)
            }
            Command::Default => match self.chat.default_city_id {
                Some(id) => {
                    let city = self.repo.search_city_by_id(&id).await?;

                    self.get_weather(city).await?;

                    Ok(None)
                }
                None => {
                    self.set_city().await?;

                    self.not_default_message().await?;

                    Ok(None)
                }
            },
            Command::Schedule => {
                self.schedule_weather().await?;
                Ok(None)
            }
            Command::SetOffset => {
                self.set_offset().await?;
                Ok(None)
            }
            Command::UnSchedule => self.unschedule().await,
            _ => Ok(None),
        }
    }

    async fn unschedule(&self) -> Result<Option<Vec<Forecast>>, BotError> {
        let vec = self
            .repo
            .delete_forecasts(&self.chat.id, self.chat.user_id)
            .await?;

        let text = "Your forecasts were unscheduled";
        self.send_message(text).await?;
        Ok(Some(vec))
    }

    async fn process_schedule_city(&self) -> Result<(), BotError> {
        self.find_city().await?;

        self.repo
            .modify_selected(&self.chat.id, self.chat.user_id, self.text.clone())
            .await?;

        self.repo
            .modify_state(
                &self.chat.id,
                self.chat.user_id,
                ClientState::ScheduleCityNumber,
            )
            .await?;

        Ok(())
    }

    async fn process_schedule_city_number(&self) -> Result<(), BotError> {
        match self.text.parse::<usize>() {
            Ok(number) => {
                let city = self
                    .repo
                    .get_city_row(&self.chat.selected.clone().unwrap(), number)
                    .await?;

                self.repo
                    .modify_selected(&self.chat.id, self.chat.user_id, format!("{}", city.id))
                    .await?;

                self.repo
                    .modify_state(&self.chat.id, self.chat.user_id, ClientState::Time)
                    .await?;

                self.schedule_weather_time_message().await
            }

            Err(_) => self.not_number_message().await,
        }
    }

    async fn process_find_city(&self) -> Result<(), BotError> {
        self.find_city().await?;

        self.repo
            .modify_selected(&self.chat.id, self.chat.user_id, self.text.clone())
            .await?;

        self.repo
            .modify_state(
                &self.chat.id,
                self.chat.user_id,
                ClientState::FindCityNumber,
            )
            .await?;

        Ok(())
    }

    async fn process_set_city(&self) -> Result<(), BotError> {
        self.find_city().await?;

        self.repo
            .modify_selected(&self.chat.id, self.chat.user_id, self.text.clone())
            .await?;

        self.repo
            .modify_state(&self.chat.id, self.chat.user_id, ClientState::SetCityNumber)
            .await?;

        Ok(())
    }

    async fn process_find_city_number(&self) -> Result<(), BotError> {
        match self.text.parse::<usize>() {
            Ok(number) => {
                let city = self
                    .repo
                    .get_city_row(&self.chat.selected.clone().unwrap(), number)
                    .await?;

                self.return_to_initial().await?;

                self.get_weather(city).await
            }

            Err(_) => self.not_number_message().await,
        }
    }

    async fn process_set_city_number(&self) -> Result<(), BotError> {
        match self.text.parse::<usize>() {
            Ok(number) => {
                let city = self
                    .repo
                    .get_city_row(&self.chat.selected.clone().unwrap(), number)
                    .await?;

                self.return_to_initial().await?;

                self.set_default_city(city).await
            }

            Err(_) => self.not_number_message().await,
        }
    }

    async fn not_valid_offset_message(&self) -> Result<(), BotError> {
        self.cancel(Some(
            "That's not a valid offset, it has to be a number in range [-11, 12].\n
            If your timezone is UTC + 2 put 2, if you have UTC - 10 put -10, 0 if you have UTC timezone.\n
            The command was cancelled"
            .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn schedule_forecast(
        &self,
        offset: i8,
        city_id: i32,
        user_hour: i8,
        minutes: i8,
    ) -> Result<(), BotError> {
        let hour_utc = if user_hour - offset < 0 {
            user_hour - offset + 24
        } else if user_hour - offset > 24 {
            user_hour - offset - 24
        } else {
            user_hour - offset
        };

        let cron_expression = format!("0 {} {} * * * *", minutes, hour_utc);

        let datetime = Repo::calculate_next_delivery(&cron_expression)?;

        // Here we should call repo.insert_forecast
        // We have to ask city_id for now default city id set
        self.repo
            .update_or_insert_forecast(
                &self.chat.id,
                self.chat.user_id,
                &city_id,
                cron_expression,
                datetime,
            )
            .await?;

        self.return_to_initial().await?;

        let minutes_pretty = if minutes < 10 {
            format!("0{}", minutes)
        } else {
            minutes.to_string()
        };

        let text = format!(
            "Weather info scheduled every day at {}:{} UTC {}",
            user_hour, minutes_pretty, offset
        );

        self.send_message(&text).await
    }

    async fn process_offset(&self) -> Result<(), BotError> {
        match self.text.parse::<i8>() {
            Ok(offset) => {
                if !(-11..=12).contains(&offset) {
                    return self.not_valid_offset_message().await;
                }

                self.repo
                    .modify_offset(&self.chat.id, self.chat.user_id, offset)
                    .await?;

                self.rechedule(offset).await?;

                let text = format!("Your offset was set to {}", offset);

                self.send_message(&text).await?;

                self.return_to_initial().await
            }

            Err(_) => self.not_valid_offset_message().await,
        }
    }

    async fn rechedule(&self, new_offset: i8) -> Result<(), BotError> {
        let forecasts = self
            .repo
            .get_forecasts_by_user(&self.chat.id, self.chat.user_id)
            .await?;

        // If user has not forecasts this loop wont be executed.
        for forecast in forecasts.into_iter() {
            // previous offset it is fetched
            let previous_offset: i8 = self.chat.offset.unwrap_or(0);

            // get the time of the forecast with cron_expression and previous offset
            // 0 {} {} * * * *
            let old_cron_expression = forecast.cron_expression;
            let vec: Vec<&str> = old_cron_expression.split(' ').collect();
            let minutes_utc = vec[1].parse::<i8>().unwrap();
            let old_hour_utc = vec[2].parse::<i8>().unwrap();

            let user_hour = if old_hour_utc + previous_offset > 24 {
                old_hour_utc + previous_offset - 24
            } else if old_hour_utc + previous_offset < 0 {
                old_hour_utc + previous_offset + 24
            } else {
                old_hour_utc + previous_offset
            };

            // Make the new cron_expression

            let hour_utc = if user_hour - new_offset < 0 {
                user_hour - new_offset + 24
            } else if user_hour - new_offset > 24 {
                user_hour - new_offset - 24
            } else {
                user_hour - new_offset
            };

            let new_cron_expression = format!("0 {} {} * * * *", minutes_utc, hour_utc);

            // Update forecast
            let next_delivery = Repo::calculate_next_delivery(&new_cron_expression)?;
            self.repo
                .update_forecast(&forecast.id, new_cron_expression, next_delivery)
                .await?;
        }

        Ok(())
    }

    async fn not_time_message(&self) -> Result<(), BotError> {
        self.cancel(Some(
            "That's not a well formatted time, it has to be formatted with this format `hour:minutes` being hour a number in range [0,23] 
            and minutes a number in range [0,59]. The command was cancelled"
            .to_string(),
        ))
        .await
    }

    fn parse_time(hour_or_minutes: &str, max_range: i8, min_range: i8) -> Result<i8, ()> {
        match hour_or_minutes.parse::<i8>() {
            Ok(number) => {
                if !(min_range..=max_range).contains(&number) {
                    Err(())
                } else {
                    Ok(number)
                }
            }
            Err(_) => Err(()),
        }
    }

    async fn process_time(&self) -> Result<(), BotError> {
        let vec: Vec<&str> = self.text.trim().split(':').collect();

        if vec.len() != 2 {
            return self.not_time_message().await;
        }

        let hour = match Self::parse_time(vec[0], 23, 0) {
            Err(_) => return self.not_time_message().await,
            Ok(number) => number,
        };

        let minutes = match Self::parse_time(vec[1], 59, 0) {
            Err(_) => return self.not_time_message().await,
            Ok(number) => number,
        };

        self.schedule_forecast(
            self.chat.offset.unwrap(),
            self.chat.selected.as_ref().unwrap().parse::<i32>().unwrap(),
            hour,
            minutes,
        )
        .await?;

        Ok(())
    }

    async fn find_city(&self) -> Result<(), BotError> {
        let vec = self.repo.get_city_by_pattern(&self.text).await?;

        if vec.is_empty() || vec.len() > 30 {
            let text = format!("Your city {} was not found. Command cancelled.", self.text);
            self.send_message(&text).await?;

            // User state will get reverted after return this error.
            // Also will prompt an Error log in server. I will consider here,
            // delete this error and just call cancel func.
            return Err(BotError::DbError(BotDbError::CityNotFoundError));
        }

        let mut i = 1;
        let mut text: String = "I found these cities. Put a number to select one\n\n".to_string();

        for row in vec {
            let name: String = row.get("name");
            let country: String = row.get("country");
            let state: String = row.get("state");
            if state.is_empty() {
                writeln!(&mut text, "{}. {},{}", i, name, country)?;
            } else {
                writeln!(&mut text, "{}. {},{},{}", i, name, country, state)?;
            }
            i += 1;
        }

        self.send_message(&text).await
    }

    async fn cancel(&self, custom_message: Option<String>) -> Result<(), BotError> {
        self.return_to_initial().await?;

        let text = match custom_message {
            Some(message) => message,
            None => "Your operation was canceled".to_string(),
        };
        self.send_message(&text).await
    }

    async fn revert_state(&self) -> Result<(), BotError> {
        self.cancel(None).await
    }

    async fn _unknown_command(&self) -> Result<(), BotError> {
        self.cancel(Some(
            "Unknown command. See /start for available commands".to_string(),
        ))
        .await
    }

    async fn return_to_initial(&self) -> Result<(), BotError> {
        self.repo
            .modify_state(&self.chat.id, self.chat.user_id, ClientState::Initial)
            .await?;

        Ok(())
    }

    async fn schedule_weather_time_message(&self) -> Result<(), BotError> {
        let text =
            "What time would you like to schedule ? (format hour:minutes in range 0-23:0-59)";

        self.send_message(text).await
    }

    async fn schedule_weather_message(&self) -> Result<(), BotError> {
        let text = "What city would you like to schedule ?";

        self.send_message(text).await
    }

    async fn schedule_weather(&self) -> Result<(), BotError> {
        match self.chat.offset {
            None => {
                // Just send message because it is in Initial state.
                self.send_message(
                    "Your can not schedule without offset set. Please execute /set_offset",
                )
                .await
            }
            Some(_) => {
                self.repo
                    .modify_state(&self.chat.id, self.chat.user_id, ClientState::ScheduleCity)
                    .await?;

                self.schedule_weather_message().await
            }
        }
    }

    async fn set_offset(&self) -> Result<(), BotError> {
        self.repo
            .modify_state(&self.chat.id, self.chat.user_id, ClientState::Offset)
            .await?;

        let text = "Do you have any offset respect UTC ?\n 
                (0 if your timezone is the same as UTC, 2 if UTC + 2 , -2 if UTC - 2, [-11,12])";

        self.send_message(text).await
    }

    async fn set_city(&self) -> Result<(), BotError> {
        self.repo
            .modify_state(&self.chat.id, self.chat.user_id, ClientState::SetCity)
            .await?;

        self.find_city_message().await
    }

    async fn not_number_message(&self) -> Result<(), BotError> {
        self.cancel(Some(
            "That's not a positive number in the range. The command was cancelled".to_string(),
        ))
        .await
    }

    async fn city_updated_message(&self) -> Result<(), BotError> {
        let text = "Your default city was updated";

        self.send_message(text).await
    }

    async fn find_city_message(&self) -> Result<(), BotError> {
        let text = "Write a city, let me see if I can find it";

        self.send_message(text).await
    }

    async fn start_message(&self) -> Result<(), BotError> {
        let text = "This bot provides weather info around the globe.\nIn order to use it put the command:\n
        /find_city Ask weather info from any city worldwide.\n
        /set_default_city Set your default city.\n
        /default Provides weather info from default city.\n
        It would be really greatful if you take a look at my GitHub, look how much work I invested into this bot.\n
        If you like this bot, consider giving me a star on GitHub or if you would like to self run it, fork the project please.\n
        <a href=\"https://github.com/pxp9/weather_bot_rust\">RustWeatherBot GitHub repo</a>";

        self.send_message(text).await
    }

    async fn get_weather(&self, city: City) -> Result<(), BotError> {
        let weather_client = WeatherApiClient::weather_client().await;

        let weather_info = weather_client.fetch(city.coord.lat, city.coord.lon).await?;

        let text = format!(
            "{},{}\nLat {} , Lon {}\n{}",
            city.name, city.country, city.coord.lat, city.coord.lon, weather_info,
        );

        self.send_message(&text).await
    }

    async fn set_default_city(&self, city: City) -> Result<(), BotError> {
        self.repo
            .modify_default_city(&self.chat.id, self.chat.user_id, &city.id)
            .await?;

        self.city_updated_message().await
    }

    async fn not_default_message(&self) -> Result<(), BotError> {
        let text = "Setting default city...";

        self.send_message(text).await
    }

    async fn send_message(&self, text: &str) -> Result<(), BotError> {
        let text_with_username = format!("Hi, {}!\n{}", self.username, text);

        self.api
            .send_message(self.chat.id, self.message_id, text_with_username)
            .await?;

        Ok(())
    }
    async fn send_typing(&self) -> Result<(), BotError> {
        self.api.send_typing(self.chat.id).await?;
        Ok(())
    }
}

impl ProcessUpdateTask {
    pub fn new(update: Update) -> Self {
        Self { update }
    }
}

#[typetag::serde]
#[async_trait]
impl AsyncRunnable for ProcessUpdateTask {
    async fn run(&self, queueable: &mut dyn AsyncQueueable) -> Result<(), FangError> {
        let processor = match UpdateProcessor::create(self.update.clone()).await {
            Ok(processor) => processor,
            Err(err) => {
                log::error!("Failed to initialize the processor {:?}", err);

                return Ok(());
            }
        };

        match processor.process().await {
            Err(error) => {
                log::error!(
                    "Failed to process the update {:?} - {:?}. Reverting...",
                    self.update,
                    error
                );

                if let Err(err) = processor.revert_state().await {
                    log::error!("Failed to revert: {:?}", err);
                }
            }

            Ok(option) => {
                if let Some(vec) = option {
                    let tasks: Vec<ScheduleWeatherTask> = vec
                        .into_iter()
                        .map(|forecast| {
                            ScheduleWeatherTask::builder()
                                .cron_expression(forecast.cron_expression)
                                .chat_id(forecast.chat_id)
                                .user_id(forecast.user_id)
                                .city_id(forecast.city_id)
                                .build()
                        })
                        .collect();

                    for task in tasks {
                        queueable.remove_task_by_metadata(&task).await?;
                    }
                }
            }
        }

        Ok(())
    }

    fn task_type(&self) -> String {
        TASK_TYPE.to_string()
    }
}
