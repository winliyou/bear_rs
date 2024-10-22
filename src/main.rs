use std::io;

use clap::crate_authors;
use regex::Regex;
use tokio::fs::File;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Command;

#[derive(Debug, serde::Serialize)]
struct CompileCommand {
    directory: String,
    command: String,
    file: String,
}

#[tokio::main(worker_threads = 6)]
async fn main() -> io::Result<()> {
    // 从命令行参数获取命令和参数
    let matches = clap::Command::new("bear_rs")
        .version("1.0")
        .author(crate_authors!(" , "))
        .about("A tool to generate compile_commands.json")
        .help_template(
            "{bin} {version} present by {author-with-newline}\
            {about}\n\n\
            {usage-heading} {usage}\n\n\
            {all-args}\n",
        )
        .override_usage("Usage: bear_rs [OPTIONS] -- [COMMAND] [ARGS]...\n\nUse `--` to separate bear_rs options from the command to be run.")
        .arg(
            clap::Arg::new("output_dir")
            .short('o')
            .long("output-dir")
            .value_name("DIR")
            .help("Sets the output directory")
            .num_args(1),
        )
        .arg(
            clap::Arg::new("command")
            .help("The command to run")
            .required(true)
            .trailing_var_arg(true)
            .num_args(1..)
            .allow_hyphen_values(true),
        )
        .get_matches();

    let output_dir = matches
        .get_one::<String>("output_dir")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let output_path = format!("{}/compile_commands.json", output_dir);

    // 获取外部命令和参数
    let command_and_args: Vec<&str> = matches
        .get_many::<String>("command")
        .unwrap()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>();

    println!("命令行参数: {:?}", command_and_args);
    let command = command_and_args[0];
    let args: Vec<&str> = command_and_args[1..].to_vec();

    // 创建输出文件
    let mut file = File::create(output_path).await?;
    let _ = file.write_all(b"[\n").await?;

    // 运行指定的命令并获取输出
    let process = Command::new(command)
        .args(&args) // 将命令行参数传递给命令
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = process.stdout.unwrap();
    let reader = BufReader::new(stdout);
    let error_reader = BufReader::new(process.stderr.unwrap());

    let compiler_regex = Regex::new(r"(/[\w/]+)?/(cc|c\+\+|gcc|g\+\+|clang|clang\+\+)\s").unwrap();
    let mut first_entry = true;

    // 读取标准输出
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        process_line(&line, &compiler_regex, &mut file, &mut first_entry).await;
    }

    // 读取标准错误
    let mut error_lines = error_reader.lines();
    while let Some(line) = error_lines.next_line().await? {
        println!("错误输出: {}", line); // 打印错误信息
    }

    let _ = file.write_all(b"\n]\n").await?;

    Ok(())
}

async fn process_line(line: &str, compiler_regex: &Regex, file: &mut File, first_entry: &mut bool) {
    if is_compile_command(line, compiler_regex) {
        println!("匹配的条件: {:?}", line);
        // 使用正则表达式匹配源文件
        let source_file_regex = Regex::new(r"(\S+\.(c|cpp|cc|cxx))\s?").unwrap();
        let source_file = source_file_regex
            .captures(line)
            .and_then(|caps| caps.get(1))
            .map_or("", |m| m.as_str())
            .to_string();

        let command = line;
        let directory = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let compile_command = CompileCommand {
            directory,
            command: command.to_string(),
            file: source_file, // 使用源文件作为file字段
        };
        let json = serde_json::to_string_pretty(&compile_command).unwrap();

        // 打印符合条件的编译命令
        println!("{}", command);

        // 逐行写入文件，处理逗号
        if *first_entry {
            *first_entry = false;
        } else {
            let _ = file.write_all(b",\n").await;
        }
        let _ = file.write_all(json.as_bytes()).await;
    } else {
        // 不匹配时打印条件和行内容
        println!("不匹配的条件: {:?}", line);
        if !line.contains(" -c ") {
            println!("原因: 不包含编译标志 '-c'");
        }
        if !line.contains(" -o ") {
            println!("原因: 不包含输出标志 '-o'");
        }
        if !(line.contains(".c")
            || line.contains(".cpp")
            || line.contains(".cc")
            || line.contains(".cxx"))
        {
            println!("原因: 不包含源文件扩展名");
        }
        if line.contains("CMakeFiles") || line.contains(".make") || line.contains("target") {
            println!("原因: 包含目标构建规则输出");
        }
        if !compiler_regex.is_match(line) {
            println!("原因: 不匹配编译器命令");
        }
    }
}

// 判断一行是否为有效的编译命令
fn is_compile_command(line: &str, compiler_regex: &Regex) -> bool {
    // 判断是否包含编译标志 "-c" 和 "-o"
    let contains_compile_flag = line.contains(" -c ");
    let contains_output_flag = line.contains(" -o ");

    // 进一步检查是否包含源文件（常见的源文件扩展名）
    let contains_source_file = line.contains(".c")
        || line.contains(".cpp")
        || line.contains(".cc")
        || line.contains(".cxx");

    // 使用正则表达式判断是否是编译器命令
    compiler_regex.is_match(line)
        && contains_compile_flag
        && contains_output_flag
        && contains_source_file
}
