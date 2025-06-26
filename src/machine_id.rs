#[cfg(target_os = "windows")]
pub mod windows {
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::sync::mpsc::{Receiver, RecvError, SendError, Sender, channel};
    use std::thread;

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Win32_BaseBoard")]
    #[serde(rename_all = "PascalCase")]
    struct BaseBoard {
        manufacturer: Option<String>,
        product: Option<String>,
        serial_number: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Win32_Processor")]
    #[serde(rename_all = "PascalCase")]
    struct Processor {
        name: Option<String>,
        processor_id: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Win32_DiskDrive")]
    #[serde(rename_all = "PascalCase")]
    struct DiskDrive {
        serial_number: Option<String>,
        model: Option<String>,
        index: u32,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Win32_DiskPartition")]
    #[serde(rename_all = "PascalCase")]
    struct DiskPartition {
        disk_index: u32,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename = "Win32_VideoController")]
    #[serde(rename_all = "PascalCase")]
    struct VideoController {
        name: Option<String>,
        adapter_compatibility: Option<String>,
        #[serde(rename = "PNPDeviceID")]
        pnp_device_id: Option<String>,
    }

    #[derive(Debug)]
    enum WMIQueryRequest {
        GetBaseboard,
        GetProcessor,
        GetDisksDerives,
        GetDiskPartitions,
        GetVideoControllers,
        Shutdown,
    }

    #[derive(Debug)]
    enum WMIQueryResult {
        Baseboard(Option<BaseBoard>),
        Processor(Option<Processor>),
        DiskDrives(Vec<DiskDrive>),
        DiskPartitions(Vec<DiskPartition>),
        VideoControllers(Vec<VideoController>),
        Error(MachineIdError),
    }

    #[derive(Debug)]
    pub enum MachineIdError {
        WMIInitialization(String),
        ChannelSend(String),
        ChannelRecv(String),
        QueryError(String),
        WorkerThreadPanicked(String),
        NoFactorsFound,
    }

    impl std::fmt::Display for MachineIdError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                MachineIdError::WMIInitialization(s) => {
                    write!(f, "WMI Initialization Error: {}", s)
                }
                MachineIdError::ChannelSend(s) => write!(f, "Channel Send Error: {}", s),
                MachineIdError::ChannelRecv(s) => write!(f, "Channel Receive Error: {}", s),
                MachineIdError::QueryError(s) => write!(f, "WMI Query Error: {}", s),
                MachineIdError::WorkerThreadPanicked(s) => {
                    write!(f, "Worker thread panicked: {}", s)
                }
                MachineIdError::NoFactorsFound => {
                    write!(f, "Could not gather any hardware factors")
                }
            }
        }
    }
    impl std::error::Error for MachineIdError {}

    // 转换 mpsc::SendError 为自定义错误
    impl<T> From<SendError<T>> for MachineIdError {
        fn from(err: SendError<T>) -> Self {
            MachineIdError::ChannelSend(err.to_string())
        }
    }
    // 转换 mpsc::RecvError 为自定义错误
    impl From<RecvError> for MachineIdError {
        fn from(err: RecvError) -> Self {
            MachineIdError::ChannelRecv(err.to_string())
        }
    }
    // 转换 WMIError (如果需要更具体的WMI错误类型)
    impl From<wmi::WMIError> for MachineIdError {
        fn from(err: wmi::WMIError) -> Self {
            MachineIdError::QueryError(format!("WMI specific error: {}", err))
        }
    }

    // 辅助函数，清理和标准化字符串
    fn sanitize_string(s: Option<String>) -> Option<String> {
        s.map(|val| val.trim().to_lowercase()).filter(|val| {
            !val.is_empty()
                && !val.contains("to be filled by o.e.m.")
                && !val.contains("default string")
                && !val.contains("none")
                && val != "00000000"
                && val != "o.e.m."
        })
    }

    fn wmi_worker_thread(
        rx_request: Receiver<WMIQueryRequest>,
        tx_response: Sender<WMIQueryResult>,
    ) {
        // 在新线程中初始化 COM
        let com_lib_result = wmi::COMLibrary::new();
        let wmi_con_result =
            com_lib_result.and_then(|com_lib| wmi::WMIConnection::new(com_lib.into()));

        let wmi_con = match wmi_con_result {
            Ok(con) => con,
            Err(e) => {
                // 如果 COM/WMI 初始化失败，通知主线程并退出
                match e {
                    wmi::WMIError::HResultError { hres } => {
                        let _ = tx_response.send(WMIQueryResult::Error(
                            MachineIdError::WMIInitialization(format!(
                                "WMI worker failed to initialize: {}({hres})",
                                windows::core::HRESULT::from_nt(hres).message()
                            )),
                        ));
                    }
                    _ => {
                        let _ = tx_response.send(WMIQueryResult::Error(
                            MachineIdError::WMIInitialization(format!(
                                "WMI worker failed to initialize: {}",
                                e
                            )),
                        ));
                    }
                }
                return;
            }
        };

        for request in rx_request {
            // 通道关闭时循环会自动结束
            let result_to_send = match request {
                WMIQueryRequest::GetBaseboard => match wmi_con.query::<BaseBoard>() {
                    Ok(results) => WMIQueryResult::Baseboard(results.into_iter().next()),
                    Err(e) => WMIQueryResult::Error(MachineIdError::QueryError(format!("Baseboard query failed: {}", e))),
                },
                WMIQueryRequest::GetProcessor => match wmi_con.query::<Processor>() {
                    Ok(results) => WMIQueryResult::Processor(results.into_iter().next()),
                    Err(e) => WMIQueryResult::Error(MachineIdError::QueryError(format!("Processor query failed: {}", e))),
                },
                WMIQueryRequest::GetDisksDerives => match wmi_con.raw_query::<DiskDrive>("SELECT SerialNumber, Model, Index, MediaType, InterfaceType FROM Win32_DiskDrive WHERE MediaType = 'Fixed hard disk media' AND InterfaceType != 'USB'") {
                    Ok(results) => WMIQueryResult::DiskDrives(results),
                    Err(e) => WMIQueryResult::Error(MachineIdError::QueryError(format!("DiskDrives query failed: {}", e))),
                },
                WMIQueryRequest::GetDiskPartitions => match wmi_con.raw_query::<DiskPartition>("SELECT BootPartition, DiskIndex FROM Win32_DiskPartition WHERE BootPartition = 'TRUE'") {
                    Ok(results) => WMIQueryResult::DiskPartitions(results),
                    Err(e) => WMIQueryResult::Error(MachineIdError::QueryError(format!("DiskPartitions query failed: {}", e))),
                },
                WMIQueryRequest::GetVideoControllers => match wmi_con.query::<VideoController>() {
                    Ok(results) => WMIQueryResult::VideoControllers(results),
                    Err(e) => WMIQueryResult::Error(MachineIdError::QueryError(format!("VideoControllers query failed: {}", e))),
                },
                WMIQueryRequest::Shutdown => {
                    break; // 退出循环，线程结束
                }
            };
            if tx_response.send(result_to_send).is_err() {
                // eprintln!("WMI worker: Failed to send response to main thread (channel closed). Shutting down.");
                break; // 主线程可能已经退出了，工作线程也应该退出
            }
        }
    }

    /// 通过 WMI 查询主板生产商、产品和序列号生产 Machine ID
    pub fn get_machine_id_with_factors() -> Result<(String, BTreeSet<String>), MachineIdError> {
        let (tx_request, rx_request) = channel::<WMIQueryRequest>();
        let (tx_response, rx_response) = channel::<WMIQueryResult>();

        let worker_handle = thread::spawn(move || {
            wmi_worker_thread(rx_request, tx_response);
        });
        let mut factors = BTreeSet::new();

        macro_rules! query_wmi {
            ($req:expr, $handler:expr) => {
                tx_request.send($req)?; // Propagates SendError as MachineIdError
                match rx_response.recv()? {
                    // Propagates RecvError as MachineIdError
                    WMIQueryResult::Error(e) => return Err(e),
                    result => $handler(result, &mut factors),
                }
            };
        }

        query_wmi!(WMIQueryRequest::GetBaseboard, |result,
                                                   factors: &mut BTreeSet<
            String,
        >| {
            if let WMIQueryResult::Baseboard(Some(bios)) = result {
                if let Some(val) = sanitize_string(bios.manufacturer) {
                    factors.insert(format!("bios_manufacturer:{}", val));
                }
                if let Some(val) = sanitize_string(bios.product) {
                    factors.insert(format!("bios_model:{}", val));
                }
                if let Some(val) = sanitize_string(bios.serial_number) {
                    factors.insert(format!("bios_serial:{}", val));
                }
            } else if let WMIQueryResult::Baseboard(None) = result {
                // Optionally log or handle case where BIOS info is empty but not an error
            }
        });
        query_wmi!(WMIQueryRequest::GetProcessor, |result,
                                                   factors: &mut BTreeSet<
            String,
        >| {
            if let WMIQueryResult::Processor(Some(cpu)) = result {
                if let Some(val) = sanitize_string(cpu.name) {
                    factors.insert(format!("cpu_name:{}", val));
                }
                if let Some(val) = sanitize_string(cpu.processor_id) {
                    factors.insert(format!("cpu_id:{}", val));
                }
            }
        });
        let mut system_disk_index = None;
        // 先查询分区，再根据分区的索引查询磁盘，目标是获取系统盘的序列化
        query_wmi!(
            WMIQueryRequest::GetDiskPartitions,
            |result, _factors: &mut BTreeSet<String>| {
                if let WMIQueryResult::DiskPartitions(partitions) = result {
                    system_disk_index = partitions.first().map(|it| it.disk_index)
                }
            }
        );
        if let Some(disk_index) = system_disk_index {
            query_wmi!(
                WMIQueryRequest::GetDisksDerives,
                |result, factors: &mut BTreeSet<String>| {
                    if let WMIQueryResult::DiskDrives(disks) = result {
                        let system_disk = disks.into_iter().find(|disk| disk.index == disk_index);
                        if let Some(disk) = system_disk {
                            if let Some(val) = sanitize_string(disk.model) {
                                factors.insert(format!("disk_model:{}", val));
                            }
                            if let Some(val) = sanitize_string(disk.serial_number) {
                                factors.insert(format!("disk_serial:{}", val));
                            }
                        }
                    }
                }
            );
        }

        query_wmi!(
            WMIQueryRequest::GetVideoControllers,
            |result, factors: &mut BTreeSet<String>| {
                if let WMIQueryResult::VideoControllers(gpus) = result {
                    for (i, vc) in gpus.into_iter().enumerate() {
                        let is_pci = vc
                            .pnp_device_id
                            .as_ref()
                            .map(|it| it.starts_with(r"PCI\VEN_"))
                            .unwrap_or(false);
                        if !is_pci {
                            continue;
                        }
                        let mut gpu_factors = Vec::new();
                        if let Some(val) = sanitize_string(vc.adapter_compatibility) {
                            gpu_factors.push(format!("gpu{}_manufacturer:{}", i, val));
                        }
                        if let Some(val) = sanitize_string(vc.name) {
                            gpu_factors.push(format!("gpu{}_model:{}", i, val));
                        }
                        if let Some(val) = sanitize_string(vc.pnp_device_id) {
                            gpu_factors.push(format!("gpu{}_pnp_id:{}", i, val));
                        }
                        if !gpu_factors.is_empty() {
                            gpu_factors.sort();
                            factors.insert(gpu_factors.join(";"));
                        }
                    }
                }
            }
        );

        if tx_request.send(WMIQueryRequest::Shutdown).is_err() {
            // 工作线程可能已经因为发送错误而提前退出了，这里记录一下但通常不认为是主流程的错误
            // eprintln!("Main thread: Failed to send Shutdown to worker, it might have already exited.");
        }

        match worker_handle.join() {
            Ok(_) => (), // Worker thread joined successfully
            Err(e) => {
                // e is Box<dyn Any + Send + 'static>, convert to string for error
                let panic_msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "Unknown panic in worker thread".to_string()
                };
                return Err(MachineIdError::WorkerThreadPanicked(panic_msg));
            }
        }

        if factors.is_empty() {
            return Err(MachineIdError::NoFactorsFound);
        }
        // println!("factors:\n{factors:?}");
        let combined_string = factors
            .iter()
            .map(|it| it.clone())
            .collect::<Vec<String>>()
            .join("|");
        let mut hasher = Sha256::new();
        hasher.update(combined_string);
        let hash = hasher.finalize();
        Ok((to_hex(&hash[..]), factors))
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|it| format!("{:02x}", it))
            .collect::<String>()
    }
}
