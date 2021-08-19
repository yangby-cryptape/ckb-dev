use std::path::Path;

use qiniu_upload::{UploadProgressInfo, UploaderBuilder};
use url::Url;

use crate::{
    config::QiniuSection,
    error::{Error, Result},
};

const UPLOAD_MIME_TYPE: &str = "application/octet-stream";
const UPLOAD_URL: &str = "https://upload.qiniup.com";

pub(crate) fn upload(cfg: &QiniuSection, file_path: &Path) -> Result<Url> {
    let object_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            let msg = format!(
                "failed to get file name from path [{}]",
                file_path.display()
            );
            Error::Qiniu(msg)
        })
        .map(|name| format!("{}{}", cfg.path_prefix, name))?;
    let uploader = UploaderBuilder::new(&cfg.access_key, &cfg.secret_key, &cfg.bucket)
        .up_urls(vec![UPLOAD_URL.to_owned()])
        .use_https(true)
        .build();
    let response = uploader
        .upload_path(file_path)
        .map_err(|err| {
            let msg = format!(
                "failed to create qiniu::Uploader({}) since {}",
                file_path.display(),
                err
            );
            Error::Qiniu(msg)
        })?
        .mime_type(UPLOAD_MIME_TYPE)
        .object_name(object_name)
        .upload_progress_callback(Box::new(|progress: &UploadProgressInfo| {
            log::trace!(
                "upload progress: upload id: {}, part number: {}, uploaded: {}",
                progress.upload_id(),
                progress.part_number(),
                progress.uploaded()
            );
            Ok(())
        }))
        .start()
        .map_err(|err| {
            let msg = format!(
                "failed to run qiniu::upload({}) since {}",
                file_path.display(),
                err
            );
            Error::Qiniu(msg)
        })?;
    let url = response
        .response_body()
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            let msg = "failed to parse qiniu response from key field".to_owned();
            Error::Qiniu(msg)
        })
        .map(|key| format!("{}/{}", &cfg.domain, key))
        .and_then(|s| {
            Url::parse(&s)
                .map_err(|err| Error::Cfg(format!("failed to parse response since {}", err)))
        })?;
    Ok(url)
}
