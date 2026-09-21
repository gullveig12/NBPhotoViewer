#define NOMINMAX
#include <windows.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <cstring>

using Microsoft::WRL::ComPtr;
struct NvJpeg { unsigned char *data; size_t length; uint32_t width, height; };
struct ComScope {
    HRESULT result = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    ~ComScope() { if(SUCCEEDED(result)) CoUninitialize(); }
};
#define CHECK_WIC(call) { HRESULT hr = (call); if(FAILED(hr)) return hr; }
static HRESULT jpeg_scaled(IWICBitmapDecoder *decoder, uint32_t edge, NvJpeg *out) {
    GUID container;
    CHECK_WIC(decoder->GetContainerFormat(&container));
    if(!IsEqualGUID(container, GUID_ContainerFormatJpeg)) return WINCODEC_ERR_UNSUPPORTEDOPERATION;
    ComPtr<IWICBitmapFrameDecode> frame;
    CHECK_WIC(decoder->GetFrame(0, &frame));
    UINT w, h;
    CHECK_WIC(frame->GetSize(&w, &h));
    if(!w || !h || !edge) return E_INVALIDARG;
    const UINT longest = std::max(w, h);
    if(longest > edge) {
        w = std::max(1u, (UINT)((uint64_t(w)*edge+longest-1)/longest));
        h = std::max(1u, (UINT)((uint64_t(h)*edge+longest-1)/longest));
    }
    ComPtr<IWICBitmapSourceTransform> transform;
    CHECK_WIC(frame.As(&transform));
    CHECK_WIC(transform->GetClosestSize(&w, &h));
    WICPixelFormatGUID format = GUID_WICPixelFormat24bppBGR;
    CHECK_WIC(transform->GetClosestPixelFormat(&format));
    if(!IsEqualGUID(format, GUID_WICPixelFormat24bppBGR)) return WINCODEC_ERR_UNSUPPORTEDPIXELFORMAT;
    BOOL supported = FALSE;
    CHECK_WIC(transform->DoesSupportTransform(WICBitmapTransformRotate0, &supported));
    if(!supported) return WINCODEC_ERR_UNSUPPORTEDOPERATION;
    const uint64_t size = uint64_t(w)*h*3;
    if(!size || size > 64*1024*1024) return E_OUTOFMEMORY;
    auto data = static_cast<unsigned char*>(std::malloc((size_t)size));
    if(!data) return E_OUTOFMEMORY;
    HRESULT hr = transform->CopyPixels(nullptr, w, h, &format, WICBitmapTransformRotate0, w*3, (UINT)size, data);
    if(FAILED(hr)) { std::free(data); return hr; }
    for(size_t i=0;i<size;i+=3) std::swap(data[i],data[i+2]);
    out->data=data;out->length=(size_t)size;out->width=w;out->height=h;
    return S_OK;
}
extern "C" int32_t nv_jpeg_scaled(const wchar_t *path, uint32_t edge, NvJpeg *out) {
    std::memset(out, 0, sizeof(*out));
    ComScope com;
    if(FAILED(com.result) && com.result != RPC_E_CHANGED_MODE) return com.result;
    ComPtr<IWICImagingFactory> factory;
    CHECK_WIC(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
    ComPtr<IWICBitmapDecoder> decoder;
    CHECK_WIC(factory->CreateDecoderFromFilename(path, nullptr, GENERIC_READ, WICDecodeMetadataCacheOnDemand, &decoder));
    return jpeg_scaled(decoder.Get(), edge, out);
}
extern "C" int32_t nv_jpeg_memory_scaled(const unsigned char *bytes, size_t length, uint32_t edge, NvJpeg *out) {
    std::memset(out, 0, sizeof(*out));
    if(!bytes || !length || length > UINT32_MAX) return E_INVALIDARG;
    ComScope com;
    if(FAILED(com.result) && com.result != RPC_E_CHANGED_MODE) return com.result;
    ComPtr<IWICImagingFactory> factory;
    CHECK_WIC(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
    ComPtr<IWICStream> stream;
    CHECK_WIC(factory->CreateStream(&stream));
    CHECK_WIC(stream->InitializeFromMemory(const_cast<BYTE*>(bytes), (DWORD)length));
    ComPtr<IWICBitmapDecoder> decoder;
    CHECK_WIC(factory->CreateDecoderFromStream(stream.Get(), nullptr, WICDecodeMetadataCacheOnDemand, &decoder));
    return jpeg_scaled(decoder.Get(), edge, out);
}
extern "C" int32_t nv_raster_scaled(const wchar_t *path, uint32_t edge, NvJpeg *out) {
    std::memset(out, 0, sizeof(*out));
    ComScope com;
    if(FAILED(com.result) && com.result != RPC_E_CHANGED_MODE) return com.result;
    ComPtr<IWICImagingFactory> factory;
    CHECK_WIC(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
    ComPtr<IWICBitmapDecoder> decoder;
    CHECK_WIC(factory->CreateDecoderFromFilename(path, nullptr, GENERIC_READ, WICDecodeMetadataCacheOnDemand, &decoder));
    GUID container;
    CHECK_WIC(decoder->GetContainerFormat(&container));
    if(!IsEqualGUID(container, GUID_ContainerFormatTiff)
       && !IsEqualGUID(container, GUID_ContainerFormatBmp)) return WINCODEC_ERR_UNSUPPORTEDOPERATION;
    ComPtr<IWICBitmapFrameDecode> frame;
    CHECK_WIC(decoder->GetFrame(0, &frame));
    UINT w, h;
    CHECK_WIC(frame->GetSize(&w, &h));
    if(!w || !h || !edge || edge > 1024 || uint64_t(w)*h > 100000000) return E_INVALIDARG;
    const UINT longest = std::max(w, h);
    if(longest > edge) {
        w = std::max(1u, (UINT)(uint64_t(w)*edge/longest));
        h = std::max(1u, (UINT)(uint64_t(h)*edge/longest));
    }
    // Premultiply before filtering, so invisible RGB cannot bleed into edges.
    ComPtr<IWICFormatConverter> converter;
    CHECK_WIC(factory->CreateFormatConverter(&converter));
    CHECK_WIC(converter->Initialize(frame.Get(), GUID_WICPixelFormat32bppPBGRA,
        WICBitmapDitherTypeNone, nullptr, 0.0, WICBitmapPaletteTypeCustom));
    ComPtr<IWICBitmapScaler> scaler;
    CHECK_WIC(factory->CreateBitmapScaler(&scaler));
    CHECK_WIC(scaler->Initialize(converter.Get(), w, h, WICBitmapInterpolationModeFant));
    const size_t pixels = size_t(w)*h;
    auto data = static_cast<unsigned char*>(std::malloc(pixels*4));
    if(!data) return E_OUTOFMEMORY;
    HRESULT hr = scaler->CopyPixels(nullptr, w*4, (UINT)(pixels*4), data);
    if(FAILED(hr)) { std::free(data); return hr; }
    for(size_t i=0;i<pixels;i++) {
        unsigned char b=data[i*4], g=data[i*4+1], r=data[i*4+2], a=data[i*4+3];
        unsigned bg=(29*(255-a)+127)/255;
        data[i*3]=(unsigned char)std::min(255u,r+bg);
        data[i*3+1]=(unsigned char)std::min(255u,g+bg);
        data[i*3+2]=(unsigned char)std::min(255u,b+bg);
    }
    out->data=data;out->length=pixels*3;out->width=w;out->height=h;
    return S_OK;
}

// Build a standard Windows bitmap from the exported JPEG, without touching the
// clipboard. Kept separate so tests can validate the payload non-destructively.
extern "C" int32_t nv_jpeg_dib(const unsigned char *bytes, size_t length, NvJpeg *out) {
    std::memset(out, 0, sizeof(*out));
    if(!bytes || !length || length > UINT32_MAX) return E_INVALIDARG;
    ComScope com;
    if(FAILED(com.result) && com.result != RPC_E_CHANGED_MODE) return com.result;
    ComPtr<IWICImagingFactory> factory;
    CHECK_WIC(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)));
    ComPtr<IWICStream> stream;
    CHECK_WIC(factory->CreateStream(&stream));
    CHECK_WIC(stream->InitializeFromMemory(const_cast<BYTE*>(bytes), (DWORD)length));
    ComPtr<IWICBitmapDecoder> decoder;
    CHECK_WIC(factory->CreateDecoderFromStream(stream.Get(), nullptr, WICDecodeMetadataCacheOnLoad, &decoder));
    ComPtr<IWICBitmapFrameDecode> frame;
    CHECK_WIC(decoder->GetFrame(0, &frame));
    UINT w, h;
    CHECK_WIC(frame->GetSize(&w, &h));
    uint64_t pixels = uint64_t(w)*h*4;
    if(!w || !h || pixels > 768*1024*1024) return E_OUTOFMEMORY;
    ComPtr<IWICFormatConverter> converter;
    CHECK_WIC(factory->CreateFormatConverter(&converter));
    CHECK_WIC(converter->Initialize(frame.Get(), GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom));
    auto data = static_cast<unsigned char*>(std::calloc(1, sizeof(BITMAPINFOHEADER)+(size_t)pixels));
    if(!data) return E_OUTOFMEMORY;
    auto header = reinterpret_cast<BITMAPINFOHEADER*>(data);
    header->biSize=sizeof(BITMAPINFOHEADER);header->biWidth=w;header->biHeight=-(LONG)h;
    header->biPlanes=1;header->biBitCount=32;header->biCompression=BI_RGB;header->biSizeImage=(DWORD)pixels;
    HRESULT hr=converter->CopyPixels(nullptr,w*4,(UINT)pixels,data+sizeof(BITMAPINFOHEADER));
    if(FAILED(hr)){std::free(data);return hr;}
    out->data=data;out->length=sizeof(BITMAPINFOHEADER)+(size_t)pixels;out->width=w;out->height=h;
    return S_OK;
}
struct GlobalBlock {
    HGLOBAL handle=nullptr;
    ~GlobalBlock(){if(handle)GlobalFree(handle);}
    bool assign(const void *data,size_t length){
        handle=GlobalAlloc(GMEM_MOVEABLE,length);if(!handle)return false;
        void *memory=GlobalLock(handle);if(!memory)return false;
        std::memcpy(memory,data,length);GlobalUnlock(handle);return true;
    }
    bool publish(UINT format){if(!SetClipboardData(format,handle))return false;handle=nullptr;return true;}
};
extern "C" int32_t nv_copy_jpeg(uintptr_t owner,const unsigned char *bytes,size_t length){
    if(!owner || !IsWindow(reinterpret_cast<HWND>(owner)))return E_INVALIDARG;
    NvJpeg dib{};
    CHECK_WIC(nv_jpeg_dib(bytes,length,&dib));
    GlobalBlock bitmap,jpeg,mime;
    bool allocated=bitmap.assign(dib.data,dib.length)&&jpeg.assign(bytes,length)&&mime.assign(bytes,length);
    std::free(dib.data);
    if(!allocated)return E_OUTOFMEMORY;
    UINT jpegFormat=RegisterClipboardFormatW(L"JPEG"),mimeFormat=RegisterClipboardFormatW(L"image/jpeg");
    if(!jpegFormat || !mimeFormat)return E_FAIL;
    bool opened=false;
    for(int retry=0;retry<8;retry++){if(OpenClipboard(reinterpret_cast<HWND>(owner))){opened=true;break;}Sleep(25);}
    if(!opened)return HRESULT_FROM_WIN32(ERROR_BUSY);
    struct Close {~Close(){CloseClipboard();}} close;
    if(!EmptyClipboard())return E_FAIL;
    if(!bitmap.publish(CF_DIB))return E_FAIL;
    if(!jpeg.publish(jpegFormat) || !mime.publish(mimeFormat))return E_FAIL;
    return S_OK;
}
