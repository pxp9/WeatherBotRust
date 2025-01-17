use serde::{Deserialize, Serialize};
use std::fmt;
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeatherForecast {
    pub cod: String,
    pub list: Vec<Forecast>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Forecast {
    pub main: Main,
    pub weather: Vec<WeatherInfo>,
    pub clouds: Clouds,
    pub pop: f32, // probability of precipitation 0-1 multiply by 100 to get percent
    pub wind: Wind,
    pub visibility: u32,
    pub dt_txt: String,
    #[serde(default)]
    pub rain: Option<Rain>,
    #[serde(default)]
    pub snow: Option<Snow>,
}

impl fmt::Display for Forecast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut weather_desc: &str = "";
        if !self.weather.is_empty() {
            weather_desc = &self.weather[0].description;
        }
        let temp = self.main.temp;
        let temp_min = self.main.temp_min;
        let temp_max = self.main.temp_max;
        let pressure = self.main.pressure;
        let humidity = self.main.humidity;
        let pop = self.pop * 100.0;
        let dt = self.dt_txt.clone();

        let st: String = format!(
        "\n==== {} ====\n🌍🌍 Weather: {}\n🌡️🌡️ Mean Temperature: {} ºC\n🧊🧊 Minimum temperature: {} ºC\n🔥🔥 Maximum temperature: {} ºC\n⛰️⛰️ Pressure: {} hPa\n💧💧 Humidity: {} %\n Rain probability: {} %",
        dt, weather_desc, temp, temp_min, temp_max, pressure, humidity, pop
	);

        write!(f, "{}", st)
    }
}

impl fmt::Display for WeatherForecast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for forecast in &self.list {
            write!(f, "{}", forecast)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Weather {
    pub coord: Coord,
    pub weather: Vec<WeatherInfo>,
    pub base: String,
    pub main: Main,
    pub visibility: u32,
    pub wind: Wind,
    pub clouds: Clouds,
    pub dt: u32,
    pub timezone: i64,
    pub id: u32,
    pub name: String,
    pub cod: u32,
}

impl fmt::Display for Weather {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut weather_desc: &str = "";
        if !self.weather.is_empty() {
            weather_desc = &self.weather[0].description;
        }
        let temp = self.main.temp;
        let temp_min = self.main.temp_min;
        let temp_max = self.main.temp_max;
        let pressure = self.main.pressure;
        let humidity = self.main.humidity;

        let st: String = format!(
        "\n🌍🌍 Weather: {}\n🌡️🌡️ Mean Temperature: {} ºC\n🧊🧊 Minimum temperature: {} ºC\n🔥🔥 Maximum temperature: {} ºC\n⛰️⛰️ Pressure: {} hPa\n💧💧 Humidity: {} %",
        weather_desc, temp, temp_min, temp_max, pressure, humidity
	);

        write!(f, "{}", st)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rain {
    #[serde(rename = "3h")]
    pub three_hour_volume: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snow {
    #[serde(rename = "3h")]
    pub three_hour_volume: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TypedBuilder)]
pub struct Coord {
    pub lon: f64,
    pub lat: f64,
}

impl fmt::Display for Coord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "( lon:{}, lat:{} )", self.lon, self.lat)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, TypedBuilder)]
pub struct City {
    pub id: i32,
    pub name: String,
    pub state: String,
    pub country: String,
    pub coord: Coord,
}

impl fmt::Display for City {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.state.is_empty() {
            write!(f, "{},{} Coords: {}", self.name, self.country, self.coord)
        } else {
            write!(
                f,
                "{},{},{} Coords: {}",
                self.name, self.country, self.state, self.coord
            )
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeatherInfo {
    pub id: u32,
    pub main: String,
    pub description: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Main {
    pub temp: f64,
    pub feels_like: f64,
    pub temp_min: f64,
    pub temp_max: f64,
    pub pressure: u32,
    pub humidity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wind {
    pub speed: f64,
    pub deg: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clouds {
    pub all: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sys {
    #[serde(rename = "type")]
    pub sys_type: u32,
    pub message: i64,
    pub country: String,
    pub sunrise: u32,
    pub sunset: u32,
}
