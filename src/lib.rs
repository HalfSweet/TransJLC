// SPDX-FileCopyrightText: 2025 HalfSweet
// SPDX-License-Identifier: Apache-2.0

#![allow(non_snake_case)]

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use regex::Regex;
use rust_embed::RustEmbed;
use zip::write::SimpleFileOptions;

use crate::FileName::*;

rust_i18n::i18n!("i18n");

mod FileName;
mod log;

#[derive(RustEmbed)]
#[folder = "Assets/"]
struct Asset;

pub enum EDA {
    Kicad,
    Protel,

    /// 自动识别
    Auto,

    /// 自定义
    Custom(String),
}

pub trait JlcTrait {
    fn new(path: String, output_path: String, eda: EDA) -> Self;

    /// 添加 “PCB下单必读.txt” 文件
    fn add_pcb_must_read(&mut self) -> Result<(), std::io::Error>;

    /// 遍历文件夹，如果找到了匹配的文件，就将它复制到指定的路径，并且重命名为JLC_STYLE
    fn copy_file(&mut self) -> Result<(), std::io::Error>;

    /// 将处理之后的文件打包为zip文件
    fn zip_file(&mut self, name: &str) -> Result<(), std::io::Error>;

    /// 完成最终输出：如果需要ZIP则只输出ZIP包，否则输出所有Gerber文件
    fn finalize_output(&mut self, create_zip: bool, zip_name: &str) -> Result<(), std::io::Error>;
}

pub struct JLC {
    /// The path to the Gerber file
    pub path: String,

    /// 输出路径
    pub output_path: String,

    /// eda software name
    pub eda: EDA,

    /// 处理之后的文件路径
    pub process_path: HashSet<PathBuf>,

    /// 是否忽略哈希孔径添加
    pub ignore_hash: bool,

    /// 是否为导入的PCB文档
    pub is_imported_pcb_doc: bool,

    /// 临时目录（用于解压ZIP文件）
    pub temp_dir: Option<tempfile::TempDir>,
}

impl JlcTrait for JLC {
    fn new(path: String, output_path: String, eda: EDA) -> Self {
        Self {
            path,
            output_path,
            eda,
            process_path: HashSet::new(),
            ignore_hash: false,
            is_imported_pcb_doc: false,
            temp_dir: None,
        }
    }

    fn add_pcb_must_read(&mut self) -> Result<(), std::io::Error> {
        const NAME: &str = "PCB下单必读.txt";
        let content = Asset::get(NAME).ok_or(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found",
        ))?;
        // 把这个文件写到工作目录
        let working_dir = self.get_working_dir();
        std::fs::create_dir_all(&working_dir)?;
        std::fs::write(working_dir.join(NAME), content.data.as_ref())?;
        self.process_path.insert(working_dir.join(NAME));
        Ok(())
    }

    fn copy_file(&mut self) -> Result<(), std::io::Error> {
        let files = std::fs::read_dir(&self.path)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, std::io::Error>>()?;

        let style = match &self.eda {
            EDA::Auto => {
                // 自动识别
                ALL_STYLE
                    .iter()
                    .find(|rule| {
                        // 我们假定所有的合法Gerber文件里面都一定包含了一个边框层，所以我们使用这个边框层来尝试判断是什么风格的EDA
                        let re = Regex::new(rule.Board_Outline).unwrap();
                        files.iter().any(|file| re.is_match(file.to_str().unwrap()))
                    })
                    .copied()
            }

            EDA::Custom(name) => {
                // 自定义
                ALL_STYLE.iter().find(|rule| rule.EDA_Name == name).copied()
            }

            EDA::Kicad => {
                // 使用KiCAD风格
                Some(&KICAD_STYLE).map(|v| &**v)
            }

            // EDA::AltiumDesigner => {
            //     // 使用Altium Designer风格
            //     Some(ALTUIM_DESIGNER_STYLE)
            // },
            _ => {
                // 直接使用指定的风格
                None
            }
        };

        if style.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No matching EDA style found",
            ));
        }

        for file in files {
            if let Some(file_style) = style {
                if file.is_file() {
                    // 匹配文件名
                    let file_name = file.file_name().unwrap().to_str().unwrap();
                    // 遍历style的所有字段
                    for (key, value) in file_style.clone() {
                        if key == "null" {
                            continue;
                        }

                        let mut file_paths: Vec<PathBuf> = vec![];
                        for value in value {
                            if Regex::new(value).unwrap().is_match(file_name) {
                                let file_path = match key {
                                    "InnerLayer" => {
                                        let mut num = 0;
                                        let re = Regex::new(r"\d+").unwrap();
                                        if let Some(caps) = re.captures(file_name) {
                                            // 获取第一个捕获组（即第一个数字）
                                            if let Some(matched) = caps.get(0) {
                                                num = matched.as_str().parse::<i32>().unwrap();
                                            }
                                        } else {
                                            return Err(std::io::Error::new(
                                                std::io::ErrorKind::NotFound,
                                                "No number found",
                                            ));
                                        }

                                        let new_file_name = JLC_STYLE
                                            .InnerLayer_Templete
                                            .replace("{0}", num.to_string().as_str())
                                            .replace("{1}", num.to_string().as_str());

                                        let file_path = self.get_working_dir().join(new_file_name);
                                        file_path
                                    }

                                    _ => {
                                        let file_path = self
                                            .get_working_dir()
                                            .join(JLC_STYLE.get(key).unwrap());
                                        file_path
                                    }
                                };
                                file_paths.push(file_path);
                            }

                            for file_path in &file_paths {
                                // 确保目录存在
                                if let Some(parent) = file_path.parent() {
                                    std::fs::create_dir_all(parent)?;
                                }
                                self.process_path.insert(file_path.clone());
                                std::fs::copy(file.clone(), file_path.clone())?;
                            }

                            // 钻孔层只复制不修改
                            const SKIP_KEYS: [&str; 3] =
                                ["NPTH_Through", "PTH_Through", "PTH_Through_Via"];
                            if SKIP_KEYS.contains(&key) {
                                continue;
                            }

                            // 获取运行时间
                            let now = chrono::Local::now();

                            for file_path in &file_paths {
                                // 在复制之后的文件的头部插入一些信息
                                let mut temp =
                                    std::fs::read_to_string(&file_path)?.replace("\r\n", "\n");
                                temp = format!(
                                    "G04 EasyEDA Pro v2.2.42.2, {}*\nG04 Gerber Generator version 0.3*\n{}",
                                    now.format("%Y-%m-%d %H:%M:%S"),
                                    temp
                                );

                                // 对KiCad风格的文件进行Dx*到G54Dx*的转换
                                let is_kicad = matches!(self.eda, EDA::Kicad)
                                    || file_style.EDA_Name == "KiCAD";
                                if is_kicad {
                                    temp = self.convert_kicad_aperture_format(temp);
                                }

                                // 对Gerber文件添加哈希孔径（跳过钻孔文件）
                                if !SKIP_KEYS.contains(&key) {
                                    temp = self.add_hash_aperture_to_gerber(temp)?;
                                }

                                std::fs::write(&file_path, temp)?;

                                // 将处理之后的文件路径保存到process_path
                                // self.process_path.insert(file_path.clone());
                            }
                        }
                    }
                }
            }
        }

        // 将PCB下单必读文件复制到输出路径
        self.add_pcb_must_read()?;

        Ok(())
    }

    fn zip_file(&mut self, name: &str) -> Result<(), std::io::Error> {
        // 确保输出目录存在
        std::fs::create_dir_all(&self.output_path)?;

        let zip_file = std::path::Path::new(&self.output_path).join(name.to_owned() + ".zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(zip_file)?);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o755);

        for file in &self.process_path {
            let file_name = file.file_name().unwrap().to_str().unwrap();
            zip.start_file(file_name, options)?;
            let content = std::fs::read(file)?;
            zip.write(&content)?;
        }

        zip.finish()?;
        Ok(())
    }

    fn finalize_output(&mut self, create_zip: bool, zip_name: &str) -> Result<(), std::io::Error> {
        if create_zip {
            // 如果需要ZIP，只创建ZIP文件
            self.zip_file(zip_name)?;
        } else {
            // 如果不需要ZIP，复制所有处理过的文件到最终输出目录
            std::fs::create_dir_all(&self.output_path)?;

            for file in &self.process_path {
                let file_name = file.file_name().unwrap();
                let dest_path = std::path::Path::new(&self.output_path).join(file_name);
                std::fs::copy(file, dest_path)?;
            }
        }
        Ok(())
    }
}

impl JLC {
    /// 检查路径是否为ZIP文件，如果是则解压到临时目录
    pub fn extract_zip_if_needed(&mut self) -> Result<(), std::io::Error> {
        let path = std::path::Path::new(&self.path);

        // 检查是否为文件且具有.zip扩展名
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("zip") {
            println!("Detected ZIP file, extracting to temporary directory...");

            // 创建临时目录
            let temp_dir = tempfile::TempDir::new()?;
            let temp_path = temp_dir.path();

            // 打开ZIP文件
            let file = std::fs::File::open(&self.path)?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            // 解压所有文件
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

                let outpath = temp_path.join(file.name());

                if file.name().ends_with('/') {
                    // 创建目录
                    std::fs::create_dir_all(&outpath)?;
                } else {
                    // 创建父目录（如果需要）
                    if let Some(p) = outpath.parent() {
                        if !p.exists() {
                            std::fs::create_dir_all(p)?;
                        }
                    }

                    // 解压文件
                    let mut outfile = std::fs::File::create(&outpath)?;
                    std::io::copy(&mut file, &mut outfile)?;
                }
            }

            // 更新路径为临时目录
            self.path = temp_path.to_string_lossy().to_string();
            self.temp_dir = Some(temp_dir);

            println!("ZIP file extracted to: {}", self.path);
        }

        Ok(())
    }

    /// 为KiCad风格文件转换Dx*格式为G54Dx*格式
    pub fn convert_kicad_aperture_format(&self, content: String) -> String {
        // 分行处理，避免使用不支持的前瞻断言
        let lines: Vec<&str> = content.split('\n').collect();
        let mut result_lines = Vec::new();
        
        // 匹配独立的Dx*格式的正则表达式
        let aperture_regex = regex::Regex::new(r"(D\d{2,4}\*)").unwrap();
        
        for line in lines {
            // 跳过已经包含%ADD或G54D的行
            if line.contains("%ADD") || line.contains("G54D") {
                result_lines.push(line.to_string());
            } else {
                // 在其他行中查找并替换Dx*为G54Dx*
                let modified_line = aperture_regex.replace_all(line, "G54$1");
                result_lines.push(modified_line.to_string());
            }
        }
        
        result_lines.join("\n")
    }

    /// 获取工作目录（临时目录优先，否则使用输出目录）
    fn get_working_dir(&self) -> PathBuf {
        if let Some(ref temp_dir) = self.temp_dir {
            temp_dir.path().to_path_buf()
        } else {
            PathBuf::from(&self.output_path)
        }
    }

    /// 向Gerber文件添加哈希孔径，用作文件指纹
    pub fn add_hash_aperture_to_gerber(&self, content: String) -> Result<String, std::io::Error> {
        use md5::{Digest, Md5};
        use rand::Rng;

        // 如果设置了忽略哈希或文件过大（>30MB），直接返回原内容
        if self.ignore_hash || content.len() > 30_000_000 {
            return Ok(content);
        }

        let lines: Vec<&str> = content.split('\n').collect();
        let aperture_regex = regex::Regex::new(r"^%ADD(\d{2,4})\D.*").unwrap();
        let aperture_macro_regex = regex::Regex::new(r"^%AD|^%AM").unwrap();

        let mut aperture_definitions = Vec::new();
        let mut aperture_numbers = Vec::new();
        let mut found_aperture = false;
        let number_max: u32 = 9999; // 设置最大孔径编号

        // 扫描前200行或直到找到非孔径定义
        for (index, line) in lines.iter().enumerate() {
            if index > 200
                && (!aperture_macro_regex.is_match(line) || index > 200 + (number_max as usize) * 2)
            {
                break;
            }

            if let Some(caps) = aperture_regex.captures(line) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse::<u32>() {
                        aperture_definitions.push(line.to_string());
                        aperture_numbers.push(num);
                        found_aperture = true;
                    }
                }
            } else if found_aperture {
                break;
            }
        }

        // 选择插入位置
        let mut rng = rand::thread_rng();
        let selection_index = std::cmp::min(
            5 + rng.gen_range(0..5),
            if aperture_numbers.len() > 1 {
                aperture_numbers.len() - 1
            } else {
                0
            },
        );

        let selection_count = if aperture_numbers.len() <= 5 {
            aperture_numbers.len()
        } else {
            selection_index
        };

        let (selected_aperture, target_number) =
            if selection_count > 0 && selection_index < aperture_definitions.len() {
                (
                    Some(aperture_definitions[selection_index].clone()),
                    aperture_numbers[selection_index],
                )
            } else {
                // 没有找到合适的孔径，使用默认值
                let default_number = if aperture_numbers.is_empty() {
                    10u32
                } else if aperture_numbers.len() <= 5 {
                    aperture_numbers.last().unwrap() + 1
                } else {
                    10u32
                };
                (None, default_number.min(number_max))
            };

        // 重新编号现有孔径（将大于等于target_number的孔径编号加1）
        let aperture_renumber_regex = regex::Regex::new(r"(?m)^(%ADD|G54D)(\d{2,4})(.*)$").unwrap();
        let renumbered_content = aperture_renumber_regex
            .replace_all(&content, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let number: u32 = caps[2].parse().unwrap_or(0);
                let suffix = &caps[3];

                if number < target_number || number == number_max {
                    caps[0].to_string()
                } else {
                    format!("{}{}{}", prefix, number + 1, suffix)
                }
            })
            .to_string();

        // 生成哈希孔径定义 - 需要转换为CRLF格式进行哈希计算
        // let renumbered_content_crlf = renumbered_content.replace('\n', "\r\n");
        let hash_content = if self.is_imported_pcb_doc {
            format!("494d{}", renumbered_content)
        } else {
            renumbered_content.clone()
        };

        // 计算MD5哈希
        let mut hasher = Md5::new();
        hasher.update(hash_content.as_str());
        let hash_result = hasher.finalize();
        let hash_hex = format!("{:x}", hash_result);

        // 取哈希的最后两位，转换为00-99的数字
        let last_two_hex = &hash_hex[hash_hex.len() - 2..];
        let hash_number = u32::from_str_radix(last_two_hex, 16).unwrap_or(0) % 100;
        let hash_suffix = format!("{:02}", hash_number);

        // 创建哈希孔径定义
        let base_size = rng.gen_range(0.0..1.0);
        let size_with_hash = format!("{:.2}{}", base_size, hash_suffix);
        let final_size = if size_with_hash.parse::<f64>().unwrap_or(0.0) == 0.0 {
            "0.0100".to_string()
        } else {
            size_with_hash
        };

        let hash_aperture = if let Some(ref selected) = selected_aperture {
            let size_regex = regex::Regex::new(r",([\d.]+)").unwrap();
            size_regex
                .replace(selected, |_: &regex::Captures| format!(",{}", final_size))
                .to_string()
        } else {
            format!("%ADD{}C,{}*%", target_number, final_size)
        };

        // 插入哈希孔径到合适位置
        let next_aperture_pattern = format!(r"(?m)^%ADD{}(\D)", target_number + 1);
        let next_aperture_regex = regex::Regex::new(&next_aperture_pattern).unwrap();

        let result = if next_aperture_regex.is_match(&renumbered_content) {
            // 在下一个孔径定义之前插入
            next_aperture_regex
                .replace(&renumbered_content, |caps: &regex::Captures| {
                    format!("{}\n%ADD{}{}", hash_aperture, target_number + 1, &caps[1])
                })
                .to_string()
        } else {
            // 在%LP或G命令之前插入
            let lines: Vec<&str> = renumbered_content.split('\n').collect();
            let mut result_lines = Vec::new();
            let mut inserted = false;
            let mut mo_found = false;

            for line in lines {
                if !mo_found && line.starts_with("%MO") {
                    mo_found = true;
                } else if mo_found
                    && !inserted
                    && (line.starts_with("%LP") || line.starts_with("G"))
                {
                    // 在这行之前插入哈希孔径
                    result_lines.push(hash_aperture.as_str());
                    inserted = true;
                }
                result_lines.push(line);
            }

            if !inserted {
                // 如果没有找到合适的位置，在文件末尾添加
                result_lines.push(hash_aperture.as_str());
            }

            result_lines.join("\n")
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {}
