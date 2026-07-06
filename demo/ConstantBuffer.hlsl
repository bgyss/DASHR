
// ConstantBufferStruct needs to match between the C version and the shader definition in ConstantBuffer.hlsl
// Must also be a multiple of 16 bytes, and vec2/3/4 must also be aligned.

cbuffer ConstantBufferStruct
{
	row_major float4x4 projectionFromCameraMatrix;
	row_major float4x4 cameraFromObjectMatrix;
	row_major float4x4 objectFromCameraMatrix;

	float heightScale;
	float heightOffset;
	float stepSize;
	float stepScale;

	row_major float4x4 boneFromObject[4];

	float3 sunDirInObject;
	float surfaceFromObjectTextureSize;

	int DebugMode;
	int LightingMode;
	float IndirectLighting;
	float heightNormalsScale;

	float deltaUVStep;
	float shadowAcneScaler;
	int padding2;
	int padding3;
};

