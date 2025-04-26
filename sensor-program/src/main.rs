use rppal::i2c::I2c;
use std::{thread, time};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct SensorData {
    ms5611: MS5611Data,
    ds18b20_1: f32,
    ds18b20_2: f32,
}

#[derive(Serialize, Deserialize, Debug)]
struct MS5611Data {
    d1: u32,
    d2: u32,
    temperature: f64,
    pressure: f64,
}

fn read_calibration_word(i2c: &mut I2c, addr: u8) -> Result<u16, Box<dyn std::error::Error>> {
    let mut buf = [0u8; 2];
    i2c.write(&[addr])?;
    thread::sleep(time::Duration::from_millis(10));
    i2c.read(&mut buf)?;
    Ok(((buf[0] as u16) << 8) | buf[1] as u16)
}

fn read_and_calculate_ms5611() -> Result<MS5611Data, Box<dyn std::error::Error>> {
    let mut i2c = I2c::with_bus(1)?;
    i2c.set_slave_address(0x77)?;

    i2c.write(&[0x48])?;
    thread::sleep(time::Duration::from_millis(50));
    i2c.write(&[0x00])?;
    let mut buf = [0u8; 3];
    i2c.read(&mut buf)?;
    let d1 = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | buf[2] as u32;

    i2c.write(&[0x58])?;
    thread::sleep(time::Duration::from_millis(50));
    i2c.write(&[0x00])?;
    i2c.read(&mut buf)?;
    let d2 = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | buf[2] as u32;

    let c1 = read_calibration_word(&mut i2c, 0xA2)? as u32;
    let c2 = read_calibration_word(&mut i2c, 0xA4)? as u32;
    let c3 = read_calibration_word(&mut i2c, 0xA6)? as u32;
    let c4 = read_calibration_word(&mut i2c, 0xA8)? as u32;
    let c5 = read_calibration_word(&mut i2c, 0xAA)? as u32;
    let c6 = read_calibration_word(&mut i2c, 0xAC)? as u32;

    let d_t = d2 as i64 - (c5 as i64 * 256);
    let temp = 2000 + (d_t * c6 as i64) / (1 << 23);
    let off = (c2 as i64) * (1 << 16) + ((c4 as i64) * d_t) / (1 << 7);
    let sens = (c1 as i64) * (1 << 15) + ((c3 as i64) * d_t) / (1 << 8);
    let press = (((d1 as i64 * sens) / (1 << 21)) - off) / (1 << 15);

    let temperature = temp as f64 / 100.0;
    let pressure = press as f64 / 100.0;

    Ok(MS5611Data { d1, d2, temperature, pressure })
}

fn read_temperature_ds18b20(sensor_id: &str) -> Result<f32, Box<dyn std::error::Error>> {
    let path = format!("/sys/bus/w1/devices/{}/w1_slave", sensor_id);
    let mut content = String::new();
    File::open(path)?.read_to_string(&mut content)?;

    if content.contains("YES") {
        let temp_pos = content.find("t=").ok_or("Valore t= non trovato")? + 2;
        let temp_str = &content[temp_pos..].trim();
        let temp_raw: f32 = temp_str.parse()?;
        Ok(temp_raw / 1000.0)
    } else {
        Err("Errore nella lettura del DS18B20".into())
    }
}

fn log_data_to_json(sensor_data: SensorData) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("sensor_data.json")?;

    let json_data = serde_json::to_string(&sensor_data)?;

    writeln!(file, "{}", json_data)?;

    Ok(())
}

fn main() {
    loop {
        match read_and_calculate_ms5611() {
            Ok(ms5611_data) => {
                println!("Raw D1 (pressione): {}", ms5611_data.d1);
                println!("Raw D2 (temperatura): {}", ms5611_data.d2);
                println!("Temperatura calcolata: {:.2} °C", ms5611_data.temperature);
                println!("Pressione calcolata: {:.2} hPa", ms5611_data.pressure);

                let sensor1 = "28-277a480a6461";
                let sensor2 = "28-7c7a480a6461";

                let ds18b20_1_temp = match read_temperature_ds18b20(sensor1) {
                    Ok(temp) => temp,
                    Err(e) => {
                        println!("Errore lettura DS18B20 1: {}", e);
                        0.0
                    }
                };

                let ds18b20_2_temp = match read_temperature_ds18b20(sensor2) {
                    Ok(temp) => temp,
                    Err(e) => {
                        println!("Errore lettura DS18B20 2: {}", e);
                        0.0
                    }
                };

                println!("Temperatura DS18B20 1: {:.2} °C", ds18b20_1_temp);
                println!("Temperatura DS18B20 2: {:.2} °C", ds18b20_2_temp);

                let sensor_data = SensorData {
                    ms5611: ms5611_data,
                    ds18b20_1: ds18b20_1_temp,
                    ds18b20_2: ds18b20_2_temp,
                };

                if let Err(e) = log_data_to_json(sensor_data) {
                    println!("Errore nel salvataggio dei dati nel file JSON: {}", e);
                }
            }
            Err(e) => println!("Errore MS5611: {}", e),
        }

        thread::sleep(time::Duration::from_secs(5));
    }
}
