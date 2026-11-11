use std::{
    fs::{self, File},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use campuslingo_ingestion::{
    ModelLease, RESERVED_HOST_BYTES, host_memory,
    ocr::{ManagedOcrConfig, ManagedOcrServer, OcrClient},
    pipeline::{ExtractionPipeline, PipelineConfig, RunStatus, default_lease_path},
    process_rss, project_root,
};
use serde_json::{Value, json};
use tokio::{
    process::Command,
    time::{Instant, sleep},
};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

fn option(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn synthetic_source_matches(spec: &campuslingo_domain::contracts::SandboxSpec) -> bool {
    use campuslingo_domain::contracts::{Archetype, Expression, MathOp};
    if spec.archetype != Archetype::Tradeoff
        || spec.modifier != "BUDGET_CAP"
        || spec.parameters.len() != 1
        || spec.targets.len() != 1
    {
        return false;
    }
    let p = &spec.parameters[0];
    let t = &spec.targets[0];
    if p.name != "power"
        || p.minimum != 0.0
        || p.maximum != 10.0
        || p.step != 1.0
        || p.initial != 0.0
        || t.lower != 12.0
        || t.upper != 14.0
    {
        return false;
    }
    let Expression::Operation {
        op: MathOp::Multiply,
        args,
    } = &t.expression
    else {
        return false;
    };
    if args.len() != 2 {
        return false;
    }
    let pair = |left: &Expression, right: &Expression| {
        matches!(left, Expression::Constant { value } if *value == 2.0)
            && matches!(right, Expression::Variable { name } if name == "power")
    };
    pair(&args[0], &args[1]) || pair(&args[1], &args[0])
}

fn validate_generated_sandbox(content: &str) -> Value {
    match serde_json::from_str::<Value>(content) {
        Ok(value) => {
            match serde_json::from_value::<campuslingo_domain::contracts::SandboxSpec>(value) {
                Ok(spec) => match campuslingo_domain::sandbox::verify(&spec) {
                    Ok(result) => {
                        json!({"valid_json":true,"sandbox":result,"source_constraints_match":synthetic_source_matches(&spec)})
                    }
                    Err(error) => json!({"valid_json":true,"error":error}),
                },
                Err(error) => json!({"valid_json":true,"error":error.to_string()}),
            }
        }
        Err(error) => json!({"valid_json":false,"error":error.to_string()}),
    }
}

async fn requests(client: &reqwest::Client, url: &str, role: &str) -> Result<Vec<Value>, Error> {
    let router_schema = json!({"type":"object", "properties":{"archetype":{"type":"string","enum":["PIPELINE","TRADEOFF","SPATIAL","CONTRAST","TRAP","AXIOM"]},"visual_3d":{"type":"boolean"}}, "required":["archetype","visual_3d"], "additionalProperties":false});
    let sandbox_schema = serde_json::to_value(schemars_schema())?;
    let prompts: Vec<(&str, Option<&str>)> = if role == "router" {
        vec![
            (
                "A pipeline has three stages; the middle stage is congested. Route flow to relieve the bottleneck.",
                Some("PIPELINE"),
            ),
            (
                "Increasing model complexity improves fit but increases memory cost. Balance the two constraints.",
                Some("TRADEOFF"),
            ),
            (
                "A byte contains eight bits. Recall this definition.",
                Some("AXIOM"),
            ),
        ]
    } else {
        vec![
            (
                "Create a TRADEOFF BUDGET_CAP sandbox. Exactly one parameter named power: label Power, minimum 0, maximum 10, step 1, initial 0. Exactly one target named output with expression 2 multiplied by power, lower 12, upper 14. schema_version 1.0.0. time_limit_ms 12000. Expression grammar in enforced key order: constant leaf {kind:constant,value:number}; variable leaf {kind:variable,name:string}; operation {args:[expression,expression],kind:operation,op:multiply}. The root expression must begin with the args key, NOT the kind key. Use one multiply with two leaves, constant 2 and variable power. No empty args or extra operation wrappers.",
                None
            );
            2
        ]
    };
    let mut results = Vec::new();
    for (prompt, expected) in prompts {
        let schema = if role == "router" {
            &router_schema
        } else {
            &sandbox_schema
        };
        let system = if role == "router" {
            "Return only JSON with archetype and visual_3d. Classify structure: PIPELINE=ordered stages/flow/bottlenecks; TRADEOFF=competing objectives/resource constraints; SPATIAL=locations/parts/geometry; CONTRAST=comparing two systems; TRAP=scientific error; AXIOM=isolated fact/definition. visual_3d is false for AXIOM, true for meaningful visual interactions. No reasoning."
        } else {
            "Return only JSON. Follow the expression grammar and enforced key order: operation objects start with args, then kind, then op. An operation has at least two arguments; constants and variables are leaves with no args field. No reasoning."
        };
        let body = json!({"model":"default", "messages":[{"role":"system","content":system},{"role":"user","content":prompt}], "temperature":0, "seed":42, "max_tokens":if role=="router" {96} else {600}, "response_format":{"type":"json_object"}, "json_schema":schema, "chat_template_kwargs":{"enable_thinking":false}});
        let started = Instant::now();
        let response = client
            .post(format!("{url}/v1/chat/completions"))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let data: Value = response.json().await?;
        if !status.is_success() {
            return Err(format!("inference HTTP {status}: {data}").into());
        }
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");
        let parsed = serde_json::from_str::<Value>(content);
        let validation = match &parsed {
            Ok(value) if role == "router" => {
                json!({"valid_json":true,"matches_synthetic_label":value["archetype"].as_str()==expected, "parsed":value})
            }
            Ok(_) => validate_generated_sandbox(content),
            Err(error) => json!({"valid_json":false,"error":error.to_string()}),
        };
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut repair = Value::Null;
        if role == "generator"
            && (validation["sandbox"]["valid"] != true
                || validation["source_constraints_match"] != true)
        {
            // One bounded repair tests the capability without hiding first-pass
            // failures or changing the source's numbers/target to make it pass.
            let mut repair_body = body.clone();
            let messages = repair_body["messages"].as_array_mut().unwrap();
            messages.push(json!({"role":"assistant","content":content}));
            messages.push(json!({"role":"user","content":"The execution/source validator rejected your expression. Keep the same parameter domain and target bounds. The source says output = 2 * power, NOT constant 2. The grammar requires args before kind in an operation. Set expression to exactly {\"args\":[{\"kind\":\"constant\",\"value\":2},{\"kind\":\"variable\",\"name\":\"power\"}],\"kind\":\"operation\",\"op\":\"multiply\"}. Return the complete corrected sandbox JSON. Do not change lower=12 or upper=14."}));
            let repair_started = Instant::now();
            let response = client
                .post(format!("{url}/v1/chat/completions"))
                .json(&repair_body)
                .send()
                .await?;
            let repair_status = response.status();
            let repaired: Value = response.json().await?;
            let repaired_content = repaired["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("");
            repair = json!({"attempt":1,"http_status":repair_status.as_u16(),"elapsed_ms":repair_started.elapsed().as_secs_f64()*1000.0,"content":repaired_content,"validation":validate_generated_sandbox(repaired_content),"usage":repaired["usage"],"note":"bounded synthetic repair only; not first-pass success or a general educational-quality claim"});
        }
        results.push(json!({"phase":role,"elapsed_ms":elapsed_ms,"usage":data["usage"],"timings":data["timings"],"finish_reason":data["choices"][0]["finish_reason"],"content":content,"validation":validation,"repair":repair}));
    }
    Ok(results)
}

fn schemars_schema() -> Value {
    let schema = campuslingo_domain::contracts::schemas()
        .into_iter()
        .find(|(name, _)| *name == "SandboxSpec")
        .unwrap()
        .1;
    serde_json::to_value(schema).unwrap()
}

async fn probe(arguments: &[String]) -> Result<Value, Error> {
    let model =
        PathBuf::from(option(arguments, "--model").ok_or("--model is required")?).canonicalize()?;
    let role = option(arguments, "--role").unwrap_or_else(|| "generator".into());
    if !["generator", "router"].contains(&role.as_str()) {
        return Err("--role must be generator or router".into());
    }
    let port: u16 = option(arguments, "--port")
        .unwrap_or_else(|| "18083".into())
        .parse()?;
    let address = format!("127.0.0.1:{port}");
    // Refuse to reuse somebody else's listener. A race also fails safely at bind.
    drop(std::net::TcpListener::bind(&address)?);
    let root = project_root();
    let directory = root.join("artifacts/private");
    fs::create_dir_all(&directory)?;
    let _lease = ModelLease::acquire(&directory.join("models.lock"), RESERVED_HOST_BYTES)?;
    let before = host_memory()?;
    if before.available_bytes < RESERVED_HOST_BYTES + model.metadata()?.len() {
        return Err("model file plus reserved host headroom exceed currently available RAM; refusing unsafe admission".into());
    }
    let log_path = directory.join(format!("llama-{role}.log"));
    let log = File::create(&log_path)?;
    let mut child = Command::new("llama-server")
        .args([
            "--model",
            model.to_str().ok_or("non-UTF8 model path")?,
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--ctx-size",
            "4096",
            "--parallel",
            "1",
            "--gpu-layers",
            "99",
            "--threads",
            "4",
            "--threads-batch",
            "4",
            "--flash-attn",
            "on",
            "--no-mmproj",
            "--no-mmproj-auto",
            "--reasoning",
            "off",
            "--no-webui",
        ])
        .env("OMP_NUM_THREADS", "4")
        .env("GGML_VK_VISIBLE_DEVICES", "0")
        .env("MESA_SHADER_CACHE_DIR", directory.join("mesa-cache"))
        .kill_on_drop(true)
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let pid = child.id().ok_or("no model process ID")?;
    let peak_rss = AtomicU64::new(0);
    let lowest_available = AtomicU64::new(before.available_bytes);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(90))
        .no_proxy()
        .build()?;
    let url = format!("http://{address}");
    let started = Instant::now();
    let workload = async {
        loop {
            if let Some(status) = child.try_wait()? {
                return Err::<_, Error>(
                    format!(
                        "model server exited {status}; inspect {}",
                        log_path.display()
                    )
                    .into(),
                );
            }
            if started.elapsed() > Duration::from_secs(120) {
                return Err("model startup exceeded 120 seconds".into());
            }
            if let Ok(response) = client.get(format!("{url}/health")).send().await
                && response.status().is_success()
            {
                break;
            }
            sleep(Duration::from_millis(250)).await;
        }
        let ready_ms = started.elapsed().as_secs_f64() * 1000.0;
        let trials = if role == "generator" {
            let mut trials = requests(&client, &url, "router").await?;
            trials.extend(requests(&client, &url, "generator").await?);
            trials
        } else {
            requests(&client, &url, &role).await?
        };
        Ok((ready_ms, trials))
    };
    let watchdog = async {
        let mut breaches = 0;
        loop {
            let available = host_memory()?.available_bytes;
            let rss = process_rss(pid)?;
            peak_rss.fetch_max(rss, Ordering::Relaxed);
            lowest_available.fetch_min(available, Ordering::Relaxed);
            breaches = if available < RESERVED_HOST_BYTES || rss > 6 * 1024 * 1024 * 1024 {
                breaches + 1
            } else {
                0
            };
            if breaches >= 2 {
                return Err::<(), Error>("owned model exceeded host-headroom/RSS budget".into());
            }
            sleep(Duration::from_millis(200)).await;
        }
    };
    let result = tokio::select! {
        result = workload => result,
        error = watchdog => Err(error.err().unwrap_or_else(||"watchdog ended".into())),
    };
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    let status = child.wait().await?;
    let after = host_memory()?;
    let mut report = json!({"role":role,"model":model,"model_file_bytes":model.metadata()?.len(),"owned_process_reaped":true,"exit_status":status.code(),"peak_process_rss_bytes":peak_rss.load(Ordering::Relaxed),"minimum_host_available_bytes":lowest_available.load(Ordering::Relaxed),"reserved_host_bytes":RESERVED_HOST_BYTES,"host_before":before,"host_after":after,"log":log_path,"notes":"Synthetic smoke/latency probe only; not a calibrated router, educational-quality, OCR, TTS, or 30-duel acceptance benchmark. RSS and unified GPU memory must not be naively added."});
    match result {
        Ok((ready_ms, trials)) => {
            report["status"] = json!("measured");
            report["startup_ms"] = json!(ready_ms);
            report["trials"] = json!(trials);
        }
        Err(error) => {
            report["status"] = json!("blocked");
            report["error"] = json!(error.to_string());
        }
    }
    fs::write(
        directory.join(format!("benchmark-{role}.json")),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    Ok(report)
}

async fn extract(arguments: &[String]) -> Result<Value, Error> {
    let pdf =
        PathBuf::from(option(arguments, "--pdf").ok_or("--pdf is required")?).canonicalize()?;
    let output = PathBuf::from(option(arguments, "--output").ok_or("--output is required")?);
    fs::create_dir_all(&output)?;
    let run_id = option(arguments, "--run-id")
        .map(|value| Uuid::parse_str(&value))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let mut config = PipelineConfig::new(output.clone());
    if let Some(dpi) = option(arguments, "--dpi") {
        config.render_dpi = dpi.parse()?;
    }

    let managed = arguments.iter().any(|argument| argument == "--managed-ocr");
    let mut server = None;
    if managed {
        let model = PathBuf::from(option(arguments, "--model").ok_or("--model is required")?)
            .canonicalize()?;
        let mmproj = PathBuf::from(option(arguments, "--mmproj").ok_or("--mmproj is required")?)
            .canonicalize()?;
        let binary = option(arguments, "--llama-server")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("llama-server"));
        let lease_path = option(arguments, "--lease")
            .map(PathBuf::from)
            .unwrap_or_else(|| default_lease_path(&output));
        let threads = option(arguments, "--threads")
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or(4);
        let started = ManagedOcrServer::start(&ManagedOcrConfig {
            binary,
            model: model.clone(),
            mmproj,
            work_dir: output.clone(),
            lease_path,
            threads,
            context_size: 4096,
        })
        .await?;
        let model_name = model
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "local-ocr".into());
        config.ocr = Some(OcrClient::new(started.client_config(model_name))?);
        server = Some(started);
    } else if let (Some(endpoint), Some(model)) = (
        option(arguments, "--ocr-endpoint"),
        option(arguments, "--ocr-model"),
    ) {
        config.ocr = Some(OcrClient::new(
            campuslingo_ingestion::ocr::OcrClientConfig::external(endpoint, model),
        )?);
    }

    let manifest = ExtractionPipeline::new(config)
        .run(&pdf, run_id, None)
        .await?;
    if let Some(server) = server {
        server.stop().await;
    }
    Ok(json!({
        "status": manifest.status,
        "run_id": manifest.run_id,
        "manifest": campuslingo_ingestion::pipeline::run_directory(&output, manifest.run_id).join("manifest.json"),
        "source_pages": manifest.source_pages,
        "render_dpi": manifest.render_dpi,
        "totals": manifest.totals,
        "model": manifest.model,
        "model_peak_rss_bytes": manifest.model_peak_rss_bytes,
        "failures": manifest.failures,
        "succeeded": manifest.status == RunStatus::Succeeded,
    }))
}

async fn run() -> Result<(), Error> {
    let arguments: Vec<String> = std::env::args().collect();
    let result = match arguments.get(1).map(String::as_str) {
        Some("probe") => probe(&arguments).await?,
        Some("extract") => extract(&arguments).await?,
        Some("environment") => json!({"host":host_memory()?,"reserved_host_bytes":RESERVED_HOST_BYTES}),
        Some("onnx-probe") => {
            #[cfg(feature="onnx")]
            { let path = PathBuf::from(option(&arguments,"--library").ok_or("--library required")?); let root=project_root(); let _lease=ModelLease::acquire(&root.join("artifacts/private/models.lock"),RESERVED_HOST_BYTES)?; match campuslingo_ingestion::probe_onnx_library(&path) {Ok(())=>json!({"status":"runtime_loaded_model_not_tested"}),Err(error)=>json!({"status":"blocked","error":error.to_string()})} }
            #[cfg(not(feature="onnx"))]
            {json!({"status":"blocked","reason":"build with --features onnx and supply a compatible local ONNX Runtime library; no automatic binary download"})}
        },
        _ => return Err("usage: campuslingo-ingestion environment | probe --model PATH --role generator|router [--port N] | extract --pdf PATH --output DIR [--run-id UUID] [--dpi N] [--managed-ocr --model PATH --mmproj PATH [--llama-server PATH] [--threads N] [--lease PATH]] | onnx-probe --library PATH".into()),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()?
        .block_on(run())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_order_matches_the_serialized_operation_schema() {
        let schema = schemars_schema();
        let variants = schema["$defs"]["Expression"]["oneOf"].as_array().unwrap();
        let operation = variants
            .iter()
            .find(|variant| variant["properties"].get("args").is_some())
            .unwrap();
        let keys: Vec<&str> = operation["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["args", "kind", "op"],
            "Regenerate prompt examples when provider property order changes"
        );
    }
    #[test]
    fn source_checks_do_not_accept_a_solvable_but_fabricated_constant() {
        let mut value: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/sandbox.valid.json"))
                .unwrap();
        assert_eq!(
            validate_generated_sandbox(&value.to_string())["source_constraints_match"],
            true
        );
        value["targets"][0]["expression"] = json!({"kind":"constant","value":13});
        let result = validate_generated_sandbox(&value.to_string());
        assert_eq!(result["sandbox"]["valid"], true);
        assert_eq!(result["source_constraints_match"], false);
    }
}
