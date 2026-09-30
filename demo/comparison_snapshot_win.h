// D3D11 staging readback and Windows built-in PNG encoding (WIC).
// Link windowscodecs.lib and ole32.lib. No image-writing dependency is added.
#pragma once
#include "comparison_snapshot_format.h"
#include <d3d11.h>
#include <wincodec.h>
#include <filesystem>
#include <fstream>
#include <vector>
#include <cstdio>
#include <cstring>
#include <cstdint>

namespace dashr_snapshot {
template<class T> class Com {
    T* p = nullptr;
public:
    ~Com() { if (p) p->Release(); }
    Com() = default;
    Com(const Com&) = delete;
    Com& operator=(const Com&) = delete;
    T** put() { return &p; }
    T* get() const { return p; }
    T* operator->() const { return p; }
};
inline void check(HRESULT hr, const char* action) {
    if (FAILED(hr)) { char code[32]; std::snprintf(code,sizeof(code)," (HRESULT 0x%08lx)",static_cast<unsigned long>(hr)); throw std::runtime_error(std::string(action)+code); }
}
class Apartment {
    HRESULT result;
public:
    Apartment() : result(CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED)) { if (result != RPC_E_CHANGED_MODE) check(result,"initialize COM"); }
    ~Apartment() { if (SUCCEEDED(result)) CoUninitialize(); }
};
inline std::filesystem::path new_directory(const char* utf8_root) {
    auto root = std::filesystem::u8path(utf8_root);
    if (root.empty()) throw std::runtime_error("comparison output folder is empty");
    std::filesystem::create_directories(root);
    SYSTEMTIME t{}; GetSystemTime(&t);
    char stamp[100];
    std::snprintf(stamp,sizeof(stamp),"comparison-%04u%02u%02uT%02u%02u%02u-%03uZ",t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute,t.wSecond,t.wMilliseconds);
    for (unsigned n=0;n<1000;++n) {
        auto dir=root/(std::string(stamp)+(n ? "-"+std::to_string(n) : ""));
        if (std::filesystem::create_directory(dir)) return dir;
    }
    throw std::runtime_error("could not allocate a unique comparison snapshot directory");
}
inline void write_manifest(const std::filesystem::path& directory, const Snapshot& snapshot) {
    const auto json = manifest(snapshot); // Validate before opening or truncating the file.
    std::ofstream file(directory/"manifest.json",std::ios::binary|std::ios::trunc);
    if (!file) throw std::runtime_error("open comparison manifest");
    file.write(json.data(),static_cast<std::streamsize>(json.size())); file.flush();
    if (!file) throw std::runtime_error("write comparison manifest");
    file.close(); if (!file) throw std::runtime_error("close comparison manifest");
}
inline void png(ID3D11Device* device, ID3D11DeviceContext* context, IDXGISwapChain* swapchain, const std::filesystem::path& path) {
    Com<ID3D11Texture2D> source, staging;
    check(swapchain->GetBuffer(0,IID_PPV_ARGS(source.put())),"get swapchain image");
    D3D11_TEXTURE2D_DESC desc{}; source->GetDesc(&desc);
    const bool rgba=desc.Format==DXGI_FORMAT_R8G8B8A8_UNORM || desc.Format==DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
    const bool bgra=desc.Format==DXGI_FORMAT_B8G8R8A8_UNORM || desc.Format==DXGI_FORMAT_B8G8R8A8_UNORM_SRGB;
    if ((!rgba && !bgra) || desc.SampleDesc.Count!=1) throw std::runtime_error("snapshot requires a single-sample RGBA8 or BGRA8 swapchain");
    const uint64_t stride=uint64_t(desc.Width)*4, count=stride*desc.Height;
    if (count>UINT_MAX || !count) throw std::runtime_error("snapshot dimensions exceed WIC limits");
    D3D11_TEXTURE2D_DESC stage_desc=desc;
    stage_desc.Usage=D3D11_USAGE_STAGING; stage_desc.BindFlags=0; stage_desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ; stage_desc.MiscFlags=0;
    check(device->CreateTexture2D(&stage_desc,nullptr,staging.put()),"create snapshot staging texture");
    context->CopyResource(staging.get(),source.get());
    std::vector<BYTE> pixels(static_cast<size_t>(count));
    D3D11_MAPPED_SUBRESOURCE mapped{};
    check(context->Map(staging.get(),0,D3D11_MAP_READ,0,&mapped),"map snapshot staging texture");
    for (UINT y=0;y<desc.Height;++y) {
        BYTE* dst=pixels.data()+y*stride;
        const BYTE* src=static_cast<const BYTE*>(mapped.pData)+static_cast<size_t>(y)*mapped.RowPitch;
        std::memcpy(dst,src,static_cast<size_t>(stride));
        if (rgba) for (UINT x=0;x<desc.Width;++x) std::swap(dst[4*x],dst[4*x+2]);
    }
    context->Unmap(staging.get(),0);
    Apartment apartment;
    Com<IWICImagingFactory> factory;
    Com<IWICStream> stream;
    Com<IWICBitmapEncoder> encoder;
    Com<IWICBitmapFrameEncode> frame;
    Com<IPropertyBag2> properties;
    try {
        check(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(factory.put())),"create WIC factory");
        check(factory->CreateStream(stream.put()),"create PNG stream");
        check(stream->InitializeFromFilename(path.c_str(),GENERIC_WRITE),"open PNG output");
        check(factory->CreateEncoder(GUID_ContainerFormatPng,nullptr,encoder.put()),"create PNG encoder");
        check(encoder->Initialize(stream.get(),WICBitmapEncoderNoCache),"initialize PNG encoder");
        check(encoder->CreateNewFrame(frame.put(),properties.put()),"create PNG frame");
        check(frame->Initialize(properties.get()),"initialize PNG frame");
        check(frame->SetSize(desc.Width,desc.Height),"set PNG dimensions");
        WICPixelFormatGUID format=GUID_WICPixelFormat32bppBGRA;
        check(frame->SetPixelFormat(&format),"set PNG pixel format");
        if (!IsEqualGUID(format,GUID_WICPixelFormat32bppBGRA)) throw std::runtime_error("PNG encoder changed the requested pixel format");
        check(frame->WritePixels(desc.Height,static_cast<UINT>(stride),static_cast<UINT>(count),pixels.data()),"write PNG pixels");
        check(frame->Commit(),"commit PNG frame"); check(encoder->Commit(),"commit PNG file");
        check(stream->Commit(STGC_DEFAULT),"flush PNG file");
    } catch (...) {
        // Release COM streams before caller removes partial output.
        throw;
    }
}
inline std::string adapter_name(ID3D11Device* device) {
    Com<IDXGIDevice> dxgi; Com<IDXGIAdapter> adapter;
    check(device->QueryInterface(IID_PPV_ARGS(dxgi.put())),"query DXGI device");
    check(dxgi->GetAdapter(adapter.put()),"query DXGI adapter");
    DXGI_ADAPTER_DESC desc{}; check(adapter->GetDesc(&desc),"query adapter description");
    const int length=WideCharToMultiByte(CP_UTF8,0,desc.Description,-1,nullptr,0,nullptr,nullptr);
    if (length<=0) throw std::runtime_error("convert adapter description");
    std::string name(static_cast<size_t>(length),'\0');
    if (!WideCharToMultiByte(CP_UTF8,0,desc.Description,-1,name.data(),length,nullptr,nullptr)) throw std::runtime_error("convert adapter description");
    name.pop_back(); return name;
}
} // namespace dashr_snapshot
