use std::env;
use std::io;

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
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: {} <命令> [参数...]", args[0]);
        return Ok(());
    }

    // 创建输出文件
    let mut file = File::create("compile_commands.json").await?;
    let _ = file.write_all(b"[\n").await?;

    // 运行指定的命令并获取输出
    let process = Command::new(&args[1])
        .args(&args[2..]) // 将命令行参数传递给命令
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
        let parts: Vec<&str> = line.split_whitespace().collect();

        // 找到输出文件的位置
        let output_file_index = parts.iter().position(|&s| s == "-o").map(|i| i + 1);

        // 找到源文件的位置（假设源文件在命令的最后）
        let source_file = parts.last().unwrap_or(&"").to_string();

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
        let json = serde_json::to_string(&compile_command).unwrap();

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
