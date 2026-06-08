# PROJECT.md

该项目是基于otherone-agent框架搭建的一个运行在桌面端的AGENT智能体应用，前端使用Tauri + react，后端使用rust，基于otherone-agent进行开发，数据库使用sqllite+localfile

otherone-agent框架的代码在C:\Users\jae\Desktop\OmniBuild\otherone\otherone-agent

前端代码的样式必须要100%按照/resource/propertypes中的原型图来复现和开发

当前后端已接入 crates.io 发布的 `otherone = "0.1.2"`。API 模型测试使用 Tauri 命令调用 `otherone::ai::invoke_model_stream`，以首个流式 chunk 的返回时间作为连接测试响应时间；会话 localfile 读取与写入通过框架的 `Otherone::set_localfile_root` 指定对话数据目录，不再切换进程 current_dir。
